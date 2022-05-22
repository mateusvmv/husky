use anyhow::Result;
use bus::{Bus, BusReader};
use delegate::delegate;
use std::{
	collections::HashMap,
	hash::Hash,
	sync::Arc,
};
use parking_lot::RwLock;

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

use super::Index;

pub struct MaterialIndex<P, I, F, B>
where
	P: View,
	F: Clone,
	B: Clone,
{
	from: Index<P, I>,
	fwd: F,
	bwd: B,
	watcher: Watcher<I, Vec<P::Value>>,
	sync: Arc<Synchronizer>,
}

impl<P, I, F, B> Clone for MaterialIndex<P, I, F, B>
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

impl<P, I, F, B> MaterialIndex<P, I, F, B>
where
	P: Watch + Sync + Send,
	I: 'static + Clone + Send + Sync + Hash + Ord,
	F: Clone
		+ View<Key = I, Value = StableVec<P::Key>>
		+ Change<Key = I, Value = StableVec<P::Key>, Insert = StableVec<P::Key>>
		+ Send
		+ Sync,
	B: Clone
		+ View<Key = <P as View>::Key, Value = StableVec<(I, usize)>>
		+ Change<
			Key = <P as View>::Key,
			Value = StableVec<(I, usize)>,
			Insert = StableVec<(I, usize)>,
		> + Send
		+ Sync,
{
	pub(crate) fn new(from: Index<P, I>, fwd: F, bwd: B) -> Self {
		let source = from.from.clone();
		let reader = source.watch();
		let indexer = Arc::clone(&from.indexer);
		let bus = Arc::new(RwLock::new(Bus::new(128)));
		let sync = Arc::new(Synchronizer::from(vec![source.sync()]));
		spawn_watcher(
			Arc::clone(&sync),
			reader,
			Arc::clone(&bus),
			cloned!(fwd, bwd, move |event| {
				let mut changed: HashMap<I, StableVec<P::Key>> = HashMap::new();
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
					let new_entries = indexer(key, value);
					for i in new_entries {
						let entry = changed.entry(i.clone()).or_insert_with(|| {
							fwd.get_ref(&i)
								.ok()
								.flatten()
								.unwrap_or_else(StableVec::new)
						});
						let position = entry.push((**key).clone());
						bwd_keys.push((i, position));
					}
				}

				// Synchronize and create events
				let mut events = Vec::with_capacity(changed.len());
				for (index, keys) in changed.into_iter() {
					if keys.is_empty() {
						fwd.remove_ref(&index)?;
						let key = Arc::new(index);
						events.push(Event::Remove { key });
					} else {
						fwd.insert_ref(&index, &keys)?;
						let keys = keys.into_vec();
						let mut values = Vec::with_capacity(keys.len());
						for key in keys {
							let value = source.get_ref(&key)?;
							if let Some(value) = value {
								values.push(value);
							}
						}
						let key = Arc::new(index);
						let value = Arc::new(values);
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
			let entries = (self.from.indexer)(&k, &v);
			let mut entry = self.bwd.entry_ref(&k)?;
			let keys = entry.or_insert_with(StableVec::new);
			// Group entries by key
			let mut map = HashMap::new();
			for i in entries {
				let entry = map.entry(i).or_insert_with(Vec::new);
				entry.push(k.clone());
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
		// For the received field to be equal to the outgoing field in the index
		// Otherwise they would never be equal, and it would wait forever on get
		self.sync.reset();
		Ok(())
	}
}

macro_rules! values_from_keys {
	($s:expr, $k:expr) => {{
		let mut values = Vec::with_capacity($k.len());
		for i in $k {
			let value = $s.get_ref(&i)?;
			if let Some(value) = value {
				values.push(value);
			}
		}
		values
	}};
}

impl<P, I, F, B> View for MaterialIndex<P, I, F, B>
where
	P: View,
	I: 'static + Clone + Send + Sync,
	F: Clone + View<Key = I, Value = StableVec<P::Key>>,
	B: View,
{
	type Key = I;
	type Value = Vec<P::Value>;
	type Iter = Box<dyn Iterator<Item = Result<(I, Vec<P::Value>)>>>;
	fn get_ref(&self, key: &I) -> Result<Option<Vec<P::Value>>> {
		self.sync.wait();
		let v = self.fwd.get_ref(key)?;
		let keys = unwrap_or_return!(v).into_vec();
		let source = &self.from.from;
		let values = values_from_keys!(source, keys);
		if values.is_empty() {
			Ok(None)
		} else {
			Ok(Some(values))
		}
	}
	fn iter(&self) -> Self::Iter {
		let source = self.from.from.clone();
		let iter = self.fwd.iter();
		Box::new(iter.map(move |r| {
			let (i, k) = r?;
			let k = k.into_vec();
			let mut v = Vec::with_capacity(k.len());
			for k in k {
				let value = source.get_ref(&k)?;
				if let Some(value) = value {
					v.push(value);
				}
			}
			Ok((i, v))
		}))
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
		let v = v
			.into_vec()
			.into_iter()
			.flat_map(|k| self.from.from.get_ref(&k))
			.flatten()
			.collect::<Vec<_>>();
		Ok(Some((k, v)))
	}
	fn get_gt_ref(&self, key: &Self::Key) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		self.sync.wait();
		let e = self.fwd.get_gt_ref(key)?;
		let e = unwrap_or_return!(e);
		let (k, v) = e;
		let v = v.into_vec();
		let v = values_from_keys!(self.from.from, v);
		Ok(Some((k, v)))
	}
	fn first(&self) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		self.sync.wait();
		let e = self.fwd.first()?;
		let e = unwrap_or_return!(e);
		let (k, v) = e;
		let v = v.into_vec();
		let v = values_from_keys!(self.from.from, v);
		Ok(Some((k, v)))
	}
	fn last(&self) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		self.sync.wait();
		let e = self.fwd.last()?;
		let e = unwrap_or_return!(e);
		let (k, v) = e;
		let v = v.into_vec();
		let v = values_from_keys!(self.from.from, v);
		Ok(Some((k, v)))
	}
	fn is_empty(&self) -> Option<bool> {
		self.from.from.is_empty()
	}
	fn range(&self, range: impl std::ops::RangeBounds<Self::Key>) -> Result<Self::Iter> {
		let source = self.from.from.clone();
		let iter = self.fwd.range(range)?;
		Ok(Box::new(iter.map(move |r| {
			let (i, k) = r?;
			let k = k.into_vec();
			let mut v = Vec::with_capacity(k.len());
			for k in k {
				let value = source.get_ref(&k)?;
				if let Some(value) = value {
					v.push(value);
				}
			}
			Ok((i, v))
		})))
	}
}
impl<P, I, F, B> Change for MaterialIndex<P, I, F, B>
where
	P: View + Change,
	I: 'static + Clone + Send + Sync,
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
      fn insert_ref(&self, key: &<Self as Change>::Key, value: &<Self as Change>::Insert) -> Result<Option<<Self as Change>::Value>>;
      fn remove_owned(&self, key: <Self as Change>::Key) -> Result<Option<<Self as Change>::Value>>;
      fn remove_ref(&self, key: &<Self as Change>::Key) -> Result<Option<<Self as Change>::Value>>;
      fn clear(&self) -> Result<()>;
    }
  }
}
impl<P, I, F, B> Watch for MaterialIndex<P, I, F, B>
where
	P: Watch,
	I: 'static + Clone + Send + Sync,
	F: Clone + View<Key = I, Value = StableVec<P::Key>>,
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

impl<P, I> Store for Index<P, I>
where
	P: Watch + Sync + Send,
	I: Serial + Hash + Ord,
	<P as View>::Key: Serial,
	StableVec<(I, usize)>: Serial,
{
	type Stored = MaterialIndex<
		P,
		I,
		Tree<I, StableVec<P::Key>>,
		Tree<<P as View>::Key, StableVec<(I, usize)>>,
	>;
	fn store(&self, name: impl Hash) -> Result<Self::Stored> {
		let db = self.from.db();
		let fwd = hash!(name, "fwd");
		let bwd = hash!(name, "bwd");
		let fwd = db.open_tree(fwd)?;
		let bwd = db.open_tree(bwd)?;
		Ok(MaterialIndex::new(self.clone(), fwd, bwd))
	}
}

impl<P, I> Load for Index<P, I>
where
	P: Watch + View + Sync + Send,
	<P as View>::Key: Ord,
	I: 'static + Clone + Send + Sync + Hash + Ord,
{
	type Loaded =
		MaterialIndex<P, I, Loaded<I, StableVec<P::Key>>, Loaded<P::Key, StableVec<(I, usize)>>>;
	fn load(&self) -> Result<Self::Loaded> {
		let fwd = Loaded::new();
		let bwd = Loaded::new();
		let res = MaterialIndex::new(self.clone(), fwd, bwd);
		res.rebuild()?;
		Ok(res)
	}
}
