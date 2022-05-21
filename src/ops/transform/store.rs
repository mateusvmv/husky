use anyhow::Result;
use bus::{Bus, BusReader};
use delegate::delegate;
use std::{
	collections::HashMap,
	hash::Hash,
	sync::{Arc, RwLock},
};

use crate::{
	macros::{cloned, hash, unwrap_or_return},
	structs::stable_vec::StableVec,
	threads::{spawn_watcher, Synchronizer},
	traits::{serial::Serial, watch::Watcher},
	wrappers::{database::Db, tree::Tree},
};

use crate::traits::{
	change::Change,
	load::{Load, Loaded},
	store::Store,
	view::View,
	watch::{Event, Watch},
};

use super::Transform;

pub struct MaterialTransform<P, K, V, F, B>
where
	P: View,
	F: Clone,
	B: Clone,
{
	from: Transform<P, K, V>,
	fwd: F,
	bwd: B,
	watcher: Watcher<K, Vec<V>>,
	sync: Arc<Synchronizer>,
}

impl<P, K, V, F, B> Clone for MaterialTransform<P, K, V, F, B>
where
	P: View,
	F: Clone,
	B: Clone,
{
	fn clone(&self) -> Self {
		Self {
			from: self.from.clone(),
			fwd: self.fwd.clone(),
			bwd: self.bwd.clone(),
			watcher: self.watcher.clone(),
			sync: Arc::clone(&self.sync),
		}
	}
}

impl<P, K, V, F, B> MaterialTransform<P, K, V, F, B>
where
	P: Watch,
	K: 'static + Clone + Send + Sync + Hash + Eq,
	V: 'static + Clone + Send + Sync,
	F: Clone
		+ View<Key = K, Value = StableVec<V>>
		+ Change<Key = K, Value = StableVec<V>, Insert = StableVec<V>>
		+ Send
		+ Sync,
	B: Clone
		+ View<Key = <P as View>::Key, Value = StableVec<(K, usize)>>
		+ Change<
			Key = <P as View>::Key,
			Value = StableVec<(K, usize)>,
			Insert = StableVec<(K, usize)>,
		> + Send
		+ Sync,
{
	pub(crate) fn new(from: Transform<P, K, V>, fwd: F, bwd: B) -> Self {
		let reader = from.from.watch();
		let transformer = Arc::clone(&from.transformer);
		let bus = Arc::new(RwLock::new(Bus::new(128)));
		let sync = Arc::new(Synchronizer::from(vec![from.from.sync()]));
		spawn_watcher(
			Arc::clone(&sync),
			reader,
			Arc::clone(&bus),
			cloned!(fwd, bwd, move |event| {
				let mut changed: HashMap<K, StableVec<V>> = HashMap::new();
				let (key, value) = match &event {
					Event::Insert { key, value } => (&*key, Some(&*value)),
					Event::Remove { key } => (&*key, None),
				};

				//Remove old entries
				let bwd_keys = bwd.get_ref(key)?.unwrap_or_default();
				bwd.remove_ref(key)?;

				for (k, position) in bwd_keys.into_vec() {
					let entry = changed
						.entry(k.clone())
						.or_insert_with(|| fwd.get_ref(&k).ok().flatten().unwrap_or_default());
					entry.remove(position);
				}

				// Add new entries
				if let Some(value) = value {
					let mut bwd_keys = bwd.entry((**key).clone())?;
					let bwd_keys = bwd_keys.or_insert_with(StableVec::new);
					let new_entries = transformer(key, value);
					for (k, v) in new_entries {
						let entry = changed.entry(k.clone()).or_insert_with(|| {
							fwd.get_ref(&k)
								.ok()
								.flatten()
								.unwrap_or_else(StableVec::new)
						});
						let position = entry.push(v);
						bwd_keys.push((k, position));
					}
				}

				// Synchronize and create events
				let mut events = Vec::with_capacity(changed.len());
				for (key, value) in changed.into_iter() {
					if value.is_empty() {
						fwd.remove_ref(&key)?;
						let key = Arc::new(key);
						events.push(Event::Remove { key });
					} else {
						fwd.insert_ref(&key, &value)?;
						let key = Arc::new(key);
						let value = Arc::new(value.into_vec());
						events.push(Event::Insert { key, value });
					}
				}

				Ok(events)
			}),
		);
		let watcher = Watcher::new(move || bus);
		Self {
			from,
			fwd,
			bwd,
			watcher,
			sync,
		}
	}
	pub fn rebuild(&self) -> Result<()> {
		self.fwd.clear()?;
		self.bwd.clear()?;
		for res in self.from.from.iter() {
			let (k, v) = res?;
			let entries = (self.from.transformer)(&k, &v);
			let mut entry = self.bwd.entry(k)?;
			let keys = entry.or_insert_with(StableVec::new);
			// Group entries by key
			let mut map = HashMap::new();
			for (k, v) in entries {
				let entry = map.entry(k).or_insert_with(Vec::new);
				entry.push(v);
			}
			// Insert all at once
			for (k, v) in map.into_iter() {
				let mut entry = self.fwd.entry_ref(&k)?;
				let values = entry.or_insert_with(StableVec::new);
				let indexes = values.extend(v.into_iter());
				keys.extend(indexes.into_iter().map(|i| (k.clone(), i)));
			}
		}
		// The sync needs to be reset
		// For the received field to be equal to the outgoing field in the transform
		// Otherwise they would never be equal, and it would wait forever on get
		self.sync.reset();
		Ok(())
	}
}

impl<P, K, V, F, B> View for MaterialTransform<P, K, V, F, B>
where
	P: View,
	K: 'static + Clone + Send + Sync,
	V: 'static + Clone + Send + Sync,
	F: Clone + View<Key = K, Value = StableVec<V>>,
	B: View,
{
	type Key = K;
	type Value = Vec<V>;
	type Iter = Box<dyn Iterator<Item = Result<(K, Vec<V>)>>>;
	fn get_ref(&self, key: &K) -> Result<Option<Vec<V>>> {
		self.sync.wait();
		let v = self.fwd.get_ref(key)?;
		let v = unwrap_or_return!(v);
		Ok(Some(v.into_vec()))
	}
	fn iter(&self) -> Self::Iter {
		Box::new(self.fwd.iter().map(|v| v.map(|(k, v)| (k, v.into_vec()))))
	}
	fn contains_key_ref(&self, key: &Self::Key) -> Result<bool> {
		self.sync.wait();
		self.fwd.contains_key_ref(key)
	}
	fn get_lt_ref(&self, key: &Self::Key) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		self.sync.wait();
		let e = self.fwd.get_lt_ref(key)?;
		let e = unwrap_or_return!(e);
		let (k, v) = e;
		Ok(Some((k, v.into_vec())))
	}
	fn get_gt_ref(&self, key: &Self::Key) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		self.sync.wait();
		let e = self.fwd.get_gt_ref(key)?;
		let e = unwrap_or_return!(e);
		let (k, v) = e;
		Ok(Some((k, v.into_vec())))
	}
	fn first(&self) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		self.sync.wait();
		let e = self.fwd.first()?;
		let e = unwrap_or_return!(e);
		let (k, v) = e;
		Ok(Some((k, v.into_vec())))
	}
	fn last(&self) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		self.sync.wait();
		let e = self.fwd.last()?;
		let e = unwrap_or_return!(e);
		let (k, v) = e;
		Ok(Some((k, v.into_vec())))
	}
	fn is_empty(&self) -> bool {
		self.from.from.is_empty()
	}
	fn range(&self, range: impl std::ops::RangeBounds<Self::Key>) -> Result<Self::Iter> {
		self.sync.wait();
		let iter = self.fwd.range(range)?;
		Ok(Box::new(iter.map(|v| v.map(|(k, v)| (k, v.into_vec())))))
	}
}
impl<P, K, V, F, B> Change for MaterialTransform<P, K, V, F, B>
where
	P: View + Change,
	K: 'static + Clone + Send + Sync,
	V: 'static + Clone + Send + Sync,
	F: 'static + Clone,
	B: 'static + Clone,
{
	type Key = <P as Change>::Key;
	type Value = <P as Change>::Value;
	type Insert = <P as Change>::Insert;
  #[rustfmt::skip]
	delegate! {
    to self.from.from {
      fn insert_owned(&self, key: Self::Key, value: Self::Insert) -> Result<Option<<Self as Change>::Value>>;
      fn insert_ref(&self, key: &Self::Key, value: &Self::Insert) -> Result<Option<<Self as Change>::Value>>;
      fn remove_owned(&self, key: <Self as Change>::Key) -> Result<Option<<Self as Change>::Value>>;
      fn remove_ref(&self, key: &<Self as Change>::Key) -> Result<Option<<Self as Change>::Value>>;
      fn clear(&self) -> Result<()>;
    }
  }
}
impl<P, K, V, F, B> Watch for MaterialTransform<P, K, V, F, B>
where
	P: Watch,
	K: 'static + Clone + Send + Sync,
	V: 'static + Clone + Send + Sync,
	F: Clone + View<Key = K, Value = StableVec<V>>,
	B: View,
{
	fn watch(&self) -> BusReader<Event<Self::Key, Self::Value>> {
		self.watcher.new_reader()
	}
	fn db(&self) -> Db {
		self.from.from.db()
	}
	fn sync(&self) -> Arc<Synchronizer> {
		Arc::clone(&self.sync)
	}
	fn wait(&self) {
		self.sync.wait()
	}
}

use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;

impl<P, K, V> Store for Transform<P, K, V>
where
	P: Watch,
	K: Serial + Hash + Eq,
	V: Serial,
	<P as View>::Key: Serial,
	StableVec<(K, usize)>: Serial,
{
	type Stored = MaterialTransform<
		P,
		K,
		V,
		Tree<K, StableVec<V>>,
		Tree<<P as View>::Key, StableVec<(K, usize)>>,
	>;
	fn store(&self, name: impl Hash) -> Result<Self::Stored> {
		let db = self.from.db();
		let fwd = hash!(name, "fwd");
		let bwd = hash!(name, "bwd");
		let fwd = db.open_tree(fwd)?;
		let bwd = db.open_tree(bwd)?;
		Ok(MaterialTransform::new(self.clone(), fwd, bwd))
	}
}

impl<P, K, V> Load for Transform<P, K, V>
where
	P: Watch + View,
	<P as View>::Key: Ord,
	K: 'static + Clone + Send + Sync + Hash + Ord,
	V: 'static + Clone + Send + Sync,
{
	type Loaded = MaterialTransform<
		P,
		K,
		V,
		Loaded<K, StableVec<V>>,
		Loaded<<P as View>::Key, StableVec<(K, usize)>>,
	>;
	fn load(&self) -> Result<Self::Loaded> {
		let fwd = Loaded::new();
		let bwd = Loaded::new();
		let res = MaterialTransform::new(self.clone(), fwd, bwd);
		res.rebuild()?;
		Ok(res)
	}
}
