use std::{
	sync::{
		atomic::{AtomicU32, Ordering::Relaxed},
		Arc, Mutex, RwLock,
	},
	thread::Thread,
};

use anyhow::Result;
use bus::{Bus, BusReader};

use crate::traits::watch::Event;

pub fn spawn(f: impl FnOnce() + Send + 'static) {
	std::thread::spawn(f);
}

pub fn spawn_listener<K, V, F>(
	synchronizer: Arc<Synchronizer>,
	mut reader: BusReader<Event<K, V>>,
	cb: F,
) where
	K: 'static + Sync + Send,
	V: 'static + Sync + Send,
	F: 'static + Fn(Event<K, V>) -> Result<u32> + Send + Sync,
{
	spawn(move || {
		while let Ok(event) = reader.recv() {
			let sent = cb(event);
			synchronizer.received();
			match sent {
				Ok(sent) => synchronizer.outgoing(sent),
				Err(e) => eprint!("Error in Husky thread {:?}", e),
			}
		}
		eprintln!("Husky thread exiting");
	});
}

pub fn spawn_watcher<K, V, E, F>(
	synchronizer: Arc<Synchronizer>,
	mut reader: BusReader<Event<K, V>>,
	bus: Arc<RwLock<Bus<E>>>,
	cb: F,
) where
	K: 'static + Sync + Send,
	V: 'static + Sync + Send,
	E: 'static + Sync + Send,
	F: 'static + Fn(Event<K, V>) -> Result<Vec<E>> + Send + Sync,
{
	spawn(move || {
		while let Ok(event) = reader.recv() {
			let events = cb(event);
			synchronizer.received();
			match events {
				Ok(events) => {
					let sent = events.len();
					synchronizer.outgoing(sent as u32);
					for event in events {
						let mut bus = bus.write().unwrap();
						bus.broadcast(event);
					}
				}
				Err(e) => eprint!("Error in Husky thread {:?}", e),
			}
		}
		eprintln!("Husky thread exiting");
	});
}

#[derive(Default, Debug)]
pub struct Synchronizer {
	source: RwLock<Vec<Arc<Synchronizer>>>,
	received: AtomicU32,
	outgoing: AtomicU32,
	waiting: Mutex<Vec<Thread>>,
}

impl Synchronizer {
	pub fn new() -> Self {
		Self::default()
	}
	pub fn from(source: Vec<Arc<Synchronizer>>) -> Self {
		let received = source.iter().map(|s| s.outgoing.load(Relaxed)).sum();
		Self {
			source: RwLock::new(source),
			received: AtomicU32::new(received),
			outgoing: AtomicU32::new(0),
			waiting: Mutex::default(),
		}
	}
	pub(crate) fn push_source(&self, source: Arc<Synchronizer>) {
		self.source.write().unwrap().push(source);
	}
	pub(crate) fn reset(&self) {
		let received = self.incoming();
		self.received.store(received, Relaxed);
	}
	fn incoming(&self) -> u32 {
		self.source
			.read()
			.unwrap()
			.iter()
			.map(|i| i.outgoing.load(Relaxed))
			.sum()
	}
	fn is_sync(&self) -> bool {
		let received = self.received.load(Relaxed);
		let incoming = self.incoming();
		let source_is_sync = self.source.read().unwrap().iter().all(|s| s.is_sync());
		let self_is_sync = received == incoming;
		source_is_sync && self_is_sync
	}
	pub(crate) fn received(&self) {
		self.received.fetch_add(1, Relaxed);
		if self.is_sync() {
			let mut waiting = self.waiting.lock().unwrap();
			for thread in waiting.drain(..) {
				thread.unpark();
			}
		}
	}
	pub(crate) fn outgoing(&self, amount: u32) {
		self.outgoing.fetch_add(amount, Relaxed);
	}
	pub fn wait(&self) {
		loop {
			if self.is_sync() {
				break;
			}
			let mut waiting = self.waiting.lock().unwrap();
			waiting.push(std::thread::current());
			drop(waiting);
			std::thread::park();
		}
	}
}
