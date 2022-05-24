use anyhow::Result;

use delegate::delegate;
use std::sync::Arc;

use crate::{
	threads::Synchronizer,
	traits::{
		change::Change,
		view::View,
		watch::{Event, Watch},
	},
	wrappers::database::Db,
};

type InsertFn<P, M> = dyn Fn(M) -> <P as Change>::Insert + Send + Sync;

/// A struct that changes the insert type.
/// You can create an [Inserter] from a [Change] struct.
///
/// # Examples
/// ```
/// # use husky::{Tree, View, Change, Operate};
/// # let db = husky::open_temp().unwrap();
/// # let tree: Tree<String, String> = db.open_tree("tree").unwrap();
/// let inserter = tree.inserter(|insert: i32| insert.to_string());
///
/// tree.insert("key", "123").unwrap();
///
/// let result = tree.get("key").unwrap();
/// assert_eq!(result, Some("123".to_string()));
///
/// inserter.insert("key", 321).unwrap();
///
/// let result = tree.get("key").unwrap();
/// assert_eq!(result, Some("321".to_string()));
/// ```
pub struct Inserter<Previous, Merge>
where
	Previous: Change,
{
	inserter: Arc<InsertFn<Previous, Merge>>,
	from: Previous,
}
impl<P: Clone + View + Change, M> Clone for Inserter<P, M> {
	fn clone(&self) -> Self {
		Self {
			inserter: Arc::clone(&self.inserter),
			from: self.from.clone(),
		}
	}
}

impl<P, Insert> Inserter<P, Insert>
where
	P: Change,
{
	pub(crate) fn new<ReduceFn>(from: P, inserter: ReduceFn) -> Self
	where
		ReduceFn: 'static + Fn(Insert) -> <P as Change>::Insert + Send + Sync,
		P: 'static + Sync + Send,
	{
		let inserter = Arc::new(inserter);
		Inserter { from, inserter }
	}
}

impl<Previous, Merge> View for Inserter<Previous, Merge>
where
	Previous: View + Change,
	Merge: 'static + Clone + Send + Sync,
{
	type Key = <Previous as View>::Key;
	type Value = <Previous as View>::Value;
	type Iter = Previous::Iter;
  #[rustfmt::skip]
	delegate!(
    to self.from {
      fn get_ref(&self, key: &Self::Key) -> Result<Option<Self::Value>>;
      fn iter(&self) -> Self::Iter;
      fn contains_key_ref(&self, key: &Self::Key) -> Result<bool>;
      fn get_lt_ref(&self, key: &Self::Key) -> Result<Option<(Self::Key, Self::Value)>>
      where
        Self::Key: Ord;
      fn get_gt_ref(&self, key: &Self::Key) -> Result<Option<(Self::Key, Self::Value)>>
      where
        Self::Key: Ord;
      fn first(&self) -> Result<Option<(Self::Key, Self::Value)>>
      where
        Self::Key: Ord;
      fn last(&self) -> Result<Option<(Self::Key, Self::Value)>>
      where
        Self::Key: Ord;
      fn is_empty(&self) -> Option<bool>;
      fn range(&self, range: impl std::ops::RangeBounds<Self::Key>) -> Result<Self::Iter>;
    }
  );
}
impl<Previous, Merge> Change for Inserter<Previous, Merge>
where
	Previous: Change,
	Merge: 'static + Clone + Send + Sync,
{
	type Key = <Previous as Change>::Key;
	type Value = <Previous as Change>::Value;
	type Insert = Merge;
	fn insert_owned(
		&self,
		key: Self::Key,
		value: Self::Insert,
	) -> Result<Option<<Self as Change>::Value>> {
		let v = (self.inserter)(value);
		self.from.insert_owned(key, v)
	}
	fn fetch_and_update(
		&self,
		key: &Self::Key,
		mut f: impl FnMut(Option<Self::Value>) -> Option<Self::Insert>,
	) -> Result<Option<Self::Value>> {
		self.from
			.fetch_and_update(key, |v| f(v).and_then(|m| Some((self.inserter)(m))))
	}
  #[rustfmt::skip]
	delegate! {
    to self.from {
      fn clear(&self) -> Result<()>;
      fn remove_owned(&self, key: <Self as Change>::Key) -> Result<Option<<Self as Change>::Value>>;
      fn remove_ref(&self, key: &<Self as Change>::Key) -> Result<Option<<Self as Change>::Value>>;
	  }
	}
}
impl<Previous, Merge> Watch for Inserter<Previous, Merge>
where
	Previous: Change + Watch,
	Merge: 'static + Clone + Send + Sync,
{
	#[rustfmt::skip]
	delegate!(
    to self.from {
      fn watch(&self) -> bus::BusReader<Event<Self::Key, Self::Value>>;
      fn db(&self) -> Db;
      fn sync(&self) -> Arc<Synchronizer>;
      fn wait(&self);
    }
  );
}
