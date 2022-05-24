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

type Mapper<K, V, M> = dyn Fn(&K, &V) -> Option<M> + Send + Sync;

/// A struct that filters option values.
/// You can create a [FilterMap] from a [View] struct.
/// # Examples
/// ```
/// # use husky::{Tree, View, Change, Operate};
/// # let db = husky::open_temp().unwrap();
/// # let tree: Tree<String, Option<u32>> = db.open_tree("tree").unwrap();
/// let filter = tree.filter_map(|_, v| *v);
///
/// tree.insert("key", Some(2u32)).unwrap();
///
/// let result = filter.get("key").unwrap();
/// // Notice that it is not Option<Option<Value>>, but Option<Value>
/// assert_eq!(result, Some(2u32));
/// ```
pub struct FilterMap<Previous, Mapped>
where
	Previous: View,
{
	mapper: Arc<Mapper<Previous::Key, Previous::Value, Mapped>>,
	from: Previous,
	watcher: Watcher<Previous::Key, Mapped>,
	sync: Arc<Synchronizer>,
}
impl<P: View, M> Clone for FilterMap<P, M> {
	fn clone(&self) -> Self {
		Self {
			mapper: Arc::clone(&self.mapper),
			from: self.from.clone(),
			watcher: self.watcher.clone(),
			sync: Arc::clone(&self.sync),
		}
	}
}

impl<P, Mapped> FilterMap<P, Mapped>
where
	P: View + Watch,
	Mapped: 'static + Clone + Send + Sync,
{
	pub(crate) fn new<Mapper>(from: P, mapper: Mapper) -> Self
	where
		Mapper: 'static + Fn(&P::Key, &P::Value) -> Option<Mapped> + Sync + Send,
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
						Event::Insert { key, value } => (Arc::clone(&key), mapper(&key, &value)),
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
		FilterMap {
			from,
			mapper,
			sync,
			watcher,
		}
	}
}

impl<Previous, Mapped> View for FilterMap<Previous, Mapped>
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
		let v = (self.mapper)(key, &v);
		Ok(v)
	}
	fn iter(&self) -> Self::Iter {
		let mapper = Arc::clone(&self.mapper);
		Box::new(
			self.from
				.iter()
				.map(move |res| {
					let (k, v) = res?;
					let m = mapper(&k, &v);
					Ok((k, m))
				})
				.filter_map(|res: Result<(Self::Key, Option<Self::Value>)>| match res {
					Ok((k, Some(v))) => Some(Ok((k, v))),
					_ => None,
				}),
		)
	}
	fn contains_key_ref(&self, key: &Self::Key) -> Result<bool> {
		let v = self.from.contains_key_ref(key)?;
		if !v {
			return Ok(false);
		};
		let v = self.from.get_ref(key)?;
		let v = if let Some(v) = v { v } else { return Ok(false) };
		let v = (self.mapper)(key, &v);
		Ok(v.is_some())
	}
	fn get_lt_ref(&self, key: &Self::Key) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		let v = self.from.get_lt_ref(key)?;
		let (k, v) = if let Some(v) = v { v } else { return Ok(None) };
		let v = (self.mapper)(key, &v);
		Ok(v.map(|v| (k, v)))
	}
	fn get_gt_ref(&self, key: &Self::Key) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		let v = self.from.get_gt_ref(key)?;
		let (k, v) = if let Some(v) = v { v } else { return Ok(None) };
		let v = (self.mapper)(key, &v);
		Ok(v.map(|v| (k, v)))
	}
	fn first(&self) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		let v = self.from.first()?;
		let (k, v) = if let Some(v) = v { v } else { return Ok(None) };
		let v = (self.mapper)(&k, &v);
		Ok(v.map(|v| (k, v)))
	}
	fn last(&self) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		let v = self.from.last()?;
		let (k, v) = if let Some(v) = v { v } else { return Ok(None) };
		let v = (self.mapper)(&k, &v);
		Ok(v.map(|v| (k, v)))
	}
	fn is_empty(&self) -> Option<bool> {
		let e = self.from.is_empty();
		if e == Some(true) {
			e
		} else {
			None
		}
	}
	fn range(&self, range: impl std::ops::RangeBounds<Self::Key>) -> Result<Self::Iter> {
		let mapper = Arc::clone(&self.mapper);
		let v = self.from.range(range)?;
		Ok(Box::new(
			v.map(move |res| {
				let (k, v) = res?;
				let m = mapper(&k, &v);
				Ok((k, m))
			})
			.filter_map(|res: Result<(Self::Key, Option<Self::Value>)>| match res {
				Ok((k, Some(v))) => Some(Ok((k, v))),
				_ => None,
			}),
		))
	}
}
impl<Previous, Mapped> Change for FilterMap<Previous, Mapped>
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
      fn fetch_and_update(
        &self,
        key: &Self::Key,
        mut f: impl FnMut(Option<Self::Value>) -> Option<Self::Insert>,
      ) -> Result<Option<Self::Value>>;
	  }
	}
}
impl<Previous, Mapped> Watch for FilterMap<Previous, Mapped>
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
