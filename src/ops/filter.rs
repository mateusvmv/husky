use anyhow::Result;
use bus::Bus;
use delegate::delegate;
use std::sync::Arc;
use parking_lot::RwLock;

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

type FilterOp<K, V> = dyn Fn(&K, &V) -> bool + Send + Sync;

/// A struct that filters values.
/// You can create a [Filter] from a [View] struct.
/// # Examples
/// ```
/// # use husky::{Tree, View, Change, Operate};
/// # let db = husky::open_temp().unwrap();
/// # let tree: Tree<String, u32> = db.open_tree("tree").unwrap();
/// let filter = tree.filter(|_, v| *v > 2);
///
/// tree.insert("key", 2u32).unwrap();
///
/// let result = filter.get("key").unwrap();
/// assert_eq!(result, None);
/// ```
pub struct Filter<Previous>
where
	Previous: View,
{
	filter: Arc<FilterOp<Previous::Key, Previous::Value>>,
	from: Previous,
	watcher: Watcher<Previous::Key, Previous::Value>,
	sync: Arc<Synchronizer>,
}
impl<P: View> Clone for Filter<P> {
	fn clone(&self) -> Self {
		Self {
			filter: Arc::clone(&self.filter),
			from: self.from.clone(),
			watcher: self.watcher.clone(),
			sync: Arc::clone(&self.sync),
		}
	}
}

impl<P> Filter<P>
where
	P: View + Watch,
{
	pub(crate) fn new<F>(from: P, filter: F) -> Self
	where
		F: 'static + Fn(&P::Key, &P::Value) -> bool + Sync + Send,
		P: 'static + Sync + Send,
	{
		let filter = Arc::new(filter);
		let sync = Arc::new(Synchronizer::from(vec![from.sync()]));
		let watcher = Watcher::new(cloned!(sync, from, filter, move || {
			let bus = Arc::new(RwLock::new(Bus::new(128)));
			let previous = from.watch();
			spawn_watcher(
				sync,
				previous,
				Arc::clone(&bus),
				cloned!(filter, move |event| {
					let (key, value) = match event {
						Event::Insert { key, value } => (Arc::clone(&key), Some(value)),
						Event::Remove { key } => (Arc::clone(&key), None),
					};
					let value = match value {
						Some(value) if filter(&key, &*value) => Some(Arc::clone(&value)),
						_ => None,
					};
					let event = match value {
						Some(value) => Event::Insert { key, value },
						_ => Event::Remove { key },
					};
					Ok(vec![event])
				}),
			);
			bus
		}));
		Filter {
			from,
			filter,
			sync,
			watcher,
		}
	}
}

impl<Previous> View for Filter<Previous>
where
	Previous: View,
{
	type Key = Previous::Key;
	type Value = Previous::Value;
	type Iter = Box<dyn Iterator<Item = Result<(Self::Key, Self::Value)>>>;
	fn get_ref(&self, key: &Self::Key) -> Result<Option<Self::Value>> {
		let v = self.from.get_ref(key)?;
		let v = unwrap_or_return!(v);
		let filter = (self.filter)(key, &v);
		if filter {
			Ok(Some(v))
		} else {
			Ok(None)
		}
	}
	fn iter(&self) -> Self::Iter {
		let filter = Arc::clone(&self.filter);
		Box::new(
			self.from
				.clone()
				.iter()
        .filter_map(move |r| {
          match r {
            Ok((k, v)) => {
              if filter(&k, &v) {
                Some(Ok((k, v)))
              } else {
                None
              }
            },
            Err(e) => Some(Err(e)),
          }
        })
		)
	}
	fn contains_key_ref(&self, key: &Self::Key) -> Result<bool> {
		let c = self.from.contains_key_ref(key)?;
		if !c {
			return Ok(false);
		};
		let v = self.from.get_ref(key)?;
		let v = if let Some(v) = v { v } else { return Ok(false) };
		let filter = (self.filter)(key, &v);
		Ok(filter)
	}
	fn get_lt_ref(&self, key: &Self::Key) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		let v = self.from.get_lt_ref(key)?;
		let v = if let Some(v) = v { v } else { return Ok(None) };
		let filter = (self.filter)(&v.0, &v.1);
		if filter {
			Ok(Some(v))
		} else {
			Ok(None)
		}
	}
	fn get_gt_ref(&self, key: &Self::Key) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		let v = self.from.get_gt_ref(key)?;
		let v = if let Some(v) = v { v } else { return Ok(None) };
		let filter = (self.filter)(&v.0, &v.1);
		if filter {
			Ok(Some(v))
		} else {
			Ok(None)
		}
	}
	fn first(&self) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		let v = self.from.first()?;
		let v = if let Some(v) = v { v } else { return Ok(None) };
		let filter = (self.filter)(&v.0, &v.1);
		if filter {
			Ok(Some(v))
		} else {
			Ok(None)
		}
	}
	fn last(&self) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		let v = self.from.last()?;
		let v = if let Some(v) = v { v } else { return Ok(None) };
		let filter = (self.filter)(&v.0, &v.1);
		if filter {
			Ok(Some(v))
		} else {
			Ok(None)
		}
	}
  /// Calling is_empty on a filter will load an iterator
	fn is_empty(&self) -> bool {
		let e = self.from.is_empty();
    if e { return true };
		self.iter().size_hint().0 == 0
	}
	fn range(&self, range: impl std::ops::RangeBounds<Self::Key>) -> Result<Self::Iter> {
		let filter = Arc::clone(&self.filter);
		let iter = self.from.range(range)?;
		Ok(Box::new(
			iter
        .filter_map(move |r| {
          match r {
            Ok((k, v)) => {
              if filter(&k, &v) {
                Some(Ok((k, v)))
              } else {
                None
              }
            },
            Err(e) => Some(Err(e)),
          }
        })
		))
	}
}
impl<Previous> Change for Filter<Previous>
where
	Previous: View + Change,
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
impl<Previous> Watch for Filter<Previous>
where
	Previous: View + Watch,
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
