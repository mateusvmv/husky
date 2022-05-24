use anyhow::Result;
use bus::Bus;
use delegate::delegate;
use parking_lot::RwLock;
use std::sync::Arc;

use crate::{
	macros::{cloned, unwrap_or_return},
	threads::{spawn_watcher, Synchronizer},
	traits::{
		change::Change,
		view::View,
		watch::{Event, Watch, Watcher},
	},
	wrappers::database::Db,
};

type Mapper<K, V, M> = dyn Fn(&K, &V) -> M + Sync + Send;

/// A struct that maps values.
/// You can create a [Map] from a [View] struct.
/// # Examples
/// ```
/// # use husky::{Tree, View, Change, Operate};
/// # let db = husky::open_temp().unwrap();
/// # let tree: Tree<String, u32> = db.open_tree("tree").unwrap();
/// let map = tree.map(|_, v: &u32| v.pow(10));
///
/// tree.insert("key", 2u32).unwrap();
///
/// let result = map.get("key").unwrap();
/// assert_eq!(result, Some(2u32.pow(10)));
/// ```
pub struct Map<Previous, Mapped>
where
	Previous: View,
{
	mapper: Arc<Mapper<Previous::Key, Previous::Value, Mapped>>,
	from: Previous,
	watcher: Watcher<Previous::Key, Mapped>,
	sync: Arc<Synchronizer>,
}
impl<P: View, Mapped> Clone for Map<P, Mapped> {
	fn clone(&self) -> Self {
		Self {
			mapper: Arc::clone(&self.mapper),
			from: self.from.clone(),
			watcher: self.watcher.clone(),
			sync: Arc::clone(&self.sync),
		}
	}
}

impl<P, Mapped> Map<P, Mapped>
where
	P: View + Watch,
	Mapped: 'static + Clone + Send + Sync,
{
	pub(crate) fn new<Mapper>(from: P, mapper: Mapper) -> Self
	where
		Mapper: 'static + Fn(&P::Key, &P::Value) -> Mapped + Sync + Send,
		P: 'static + Sync + Send,
	{
		let mapper = Arc::new(mapper);
		let sync = Arc::new(Synchronizer::from(vec![from.sync()]));
		let watcher = Watcher::new(cloned!(sync, from, mapper, move || {
			let bus = Arc::new(RwLock::new(Bus::new(128)));
			let previous = from.watch();
			spawn_watcher(
				sync,
				previous,
				Arc::clone(&bus),
				cloned!(mapper, move |event| {
					let (key, value) = match event {
						Event::Insert { key, value } => {
							(Arc::clone(&key), Some(mapper(&*key, &*value)))
						}
						Event::Remove { key } => (Arc::clone(&key), None),
					};
					let value = value.map(Arc::new);
					let event = match value {
						Some(value) => Event::Insert { key, value },
						None => Event::Remove { key },
					};
					Ok(vec![event])
				}),
			);
			bus
		}));
		Map {
			from,
			mapper,
			sync,
			watcher,
		}
	}
}

impl<Previous, Mapped> View for Map<Previous, Mapped>
where
	Previous: View,
	Mapped: 'static + Clone + Send + Sync,
{
	type Key = Previous::Key;
	type Value = Mapped;
	type Iter = Box<dyn Iterator<Item = Result<(Self::Key, Self::Value)>>>;
	fn get_ref(&self, key: &Self::Key) -> Result<Option<Self::Value>> {
		let v = self.from.get_ref(key)?;
		let v = unwrap_or_return!(v);
		Ok(Some((self.mapper)(key, &v)))
	}
	fn iter(&self) -> Self::Iter {
		let mapper = Arc::clone(&self.mapper);
		Box::new(self.from.iter().map(move |res| {
			let (k, v) = res?;
			let m = mapper(&k, &v);
			Ok((k, m))
		}))
	}
	fn get_lt_ref(&self, key: &Self::Key) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		let v = self.from.get_lt_ref(key)?;
		let (k, v) = unwrap_or_return!(v);
		let v = (self.mapper)(&k, &v);
		Ok(Some((k, v)))
	}
	fn get_gt_ref(&self, key: &Self::Key) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		let v = self.from.get_gt_ref(key)?;
		let (k, v) = unwrap_or_return!(v);
		let v = (self.mapper)(&k, &v);
		Ok(Some((k, v)))
	}
	fn first(&self) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		let v = self.from.first()?;
		let (k, v) = unwrap_or_return!(v);
		let v = (self.mapper)(&k, &v);
		Ok(Some((k, v)))
	}
	fn last(&self) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		let v = self.from.last()?;
		let (k, v) = unwrap_or_return!(v);
		let v = (self.mapper)(&k, &v);
		Ok(Some((k, v)))
	}
	fn range(&self, range: impl std::ops::RangeBounds<Self::Key>) -> Result<Self::Iter> {
		let mapper = Arc::clone(&self.mapper);
		let iter = self.from.range(range)?;
		Ok(Box::new(iter.map(move |res| {
			let (k, v) = res?;
			let m = mapper(&k, &v);
			Ok((k, m))
		})))
	}
  #[rustfmt::skip]
	delegate! {
    to self.from {
      fn contains_key_ref(&self, key: &Self::Key) -> Result<bool>;
      fn is_empty(&self) -> Option<bool>;
    }
  }
}
impl<Previous, Mapped> Change for Map<Previous, Mapped>
where
	Previous: View + Change,
	Mapped: 'static + Clone + Send + Sync,
{
	type Key = <Previous as Change>::Key;
	type Value = <Previous as Change>::Value;
	type Insert = <Previous as Change>::Insert;
  #[rustfmt::skip]
	delegate! {
	  to self.from {
      fn insert_owned(&self, key: Self::Key, value: Self::Insert) -> Result<Option<<Self as Change>::Value>>;
      fn insert_ref(&self, key: &Self::Key, value: &Self::Insert) -> Result<Option<<Self as Change>::Value>>;
      fn remove_owned(&self, key: <Self as Change>::Key) -> Result<Option<<Self as Change>::Value>>;
      fn remove_ref(&self, key: &<Self as Change>::Key) -> Result<Option<<Self as Change>::Value>>;
      fn clear(&self) -> Result<()>;
	  }
	}
}
impl<Previous, Mapped> Watch for Map<Previous, Mapped>
where
	Previous: View + Watch,
	Mapped: 'static + Clone + Send + Sync,
{
	fn watch(&self) -> bus::BusReader<Event<Self::Key, Self::Value>> {
		self.watcher.new_reader()
	}
	fn db(&self) -> Db {
		self.from.db()
	}
	fn sync(&self) -> Arc<Synchronizer> {
		Arc::clone(&self.sync)
	}
	fn wait(&self) {
		self.from.wait()
	}
}
