use bus::{Bus, BusReader};
use std::sync::Arc;
use parking_lot::{RwLock, Mutex};

use crate::{threads::Synchronizer, wrappers::database::Db};

use super::view::View;

/// An event that ocurred in a tree.
#[derive(Debug)]
pub enum Event<Key, Value> {
	/// A key-value insertion
	Insert {
		/// The key where the value has been inserted
		key: Arc<Key>,
		/// The value that has been inserted
		value: Arc<Value>,
	},
	/// A key removal
	Remove {
		/// The key which value has been removed
		key: Arc<Key>,
	},
}
impl<K, V> Clone for Event<K, V> {
	fn clone(&self) -> Self {
		match self {
			Self::Insert { key, value } => Self::Insert {
				key: Arc::clone(key),
				value: Arc::clone(value),
			},
			Self::Remove { key } => Self::Remove {
				key: Arc::clone(key),
			},
		}
	}
}

/// A function that creates an event bus.
pub type Generator<K, V> = dyn FnOnce() -> Arc<RwLock<Bus<Event<K, V>>>> + Send + Sync;
/// A reference-counted mutex, for interior mutability.
pub type IntMut<T> = Arc<Mutex<T>>;
/// A reference-counted read-write lock, for shared access.
pub type Shared<T> = Arc<RwLock<T>>;
/// A bus for events.
pub type Broadcaster<K, V> = Bus<Event<K, V>>;
/// An optional [Generator]
pub type OptGenerator<K, V> = Option<Box<Generator<K, V>>>;
pub(crate) struct Watcher<Key, Value> {
	bus: IntMut<Option<Shared<Broadcaster<Key, Value>>>>,
	init: IntMut<OptGenerator<Key, Value>>,
}

impl<K, V> Clone for Watcher<K, V> {
	fn clone(&self) -> Self {
		Self {
			bus: Arc::clone(&self.bus),
			init: Arc::clone(&self.init),
		}
	}
}

impl<K, V> Watcher<K, V> {
	pub fn new<F>(init: F) -> Self
	where
		F: FnOnce() -> Arc<RwLock<Bus<Event<K, V>>>> + 'static + Send + Sync,
	{
		let b = Box::new(init);
		let init = Arc::default();
		let bus = Arc::default();
		let s = Self { bus, init };
		*s.init.lock() = Some(b);
		s
	}
	pub fn new_reader(&self) -> BusReader<Event<K, V>> {
		self.bus
			.lock()
			.get_or_insert_with(|| {
				let init = self.init.lock().take().unwrap();
				init()
			})
			.write()
			.add_rx()
	}
	pub fn send(&self, event: Event<K, V>) {
		if let Some(bus) = &*self.bus.lock() {
			let mut bus = bus.write();
			bus.broadcast(event);
		};
	}
}

/// Allows for monitoring of changes to a tree.
pub trait Watch
where
	Self: View,
{
	/// Returns a reader for the bus.
	fn watch(&self) -> BusReader<Event<Self::Key, Self::Value>>;
	/// The database where the tree is stored.
	fn db(&self) -> Db;
	/// A synchronizer for the tree.
	fn sync(&self) -> Arc<Synchronizer>;
	/// Waits until all events are processed.
	fn wait(&self);
}
