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

type ReduceFn<P, M> = dyn Fn(Option<<P as View>::Value>, M) -> <P as Change>::Insert + Send + Sync;

/// A struct that reduces values on insert.
/// You can create a [Reducer] from a [Change] struct.
/// # Important
/// If you perform an insert that bypasses the [Reducer] struct, be in on the tree or in another reduce, you may experience data races.
///
/// # Examples
/// ```
/// # use husky::{Tree, View, Change, Operate};
/// # let db = husky::open_temp().unwrap();
/// # let tree: Tree<String, String> = db.open_tree("tree").unwrap();
/// let reducer = tree.reducer(|str, insert: String| format!("{}, {}!", str.unwrap(), insert));
///
/// tree.insert("key", "hello").unwrap();
/// reducer.insert("key", "world").unwrap();
///
/// let result = reducer.get("key").unwrap();
/// assert_eq!(result, Some("hello, world!".to_string()));
/// ```
pub struct Reducer<Previous, Merge>
where
	Previous: View + Change,
{
	reducer: Arc<ReduceFn<Previous, Merge>>,
	from: Previous,
}
impl<P: Clone + View + Change, M> Clone for Reducer<P, M> {
	fn clone(&self) -> Self {
		Self {
			reducer: Arc::clone(&self.reducer),
			from: self.from.clone(),
		}
	}
}

impl<P, Merge> Reducer<P, Merge>
where
	P: View + Change,
{
	pub(crate) fn new<ReduceFn>(from: P, reducer: ReduceFn) -> Self
	where
		ReduceFn:
			'static + Fn(Option<<P as View>::Value>, Merge) -> <P as Change>::Insert + Send + Sync,
		P: 'static + Sync + Send,
	{
		let reducer = Arc::new(reducer);
		Reducer { from, reducer }
	}
}

impl<Previous, Merge> View for Reducer<Previous, Merge>
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
    }
  );
}
impl<Previous, Merge> Change for Reducer<Previous, Merge>
where
	Previous: View + Change<Key = <Previous as View>::Key>,
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
		let v = self.from.get_ref(&key)?;
		let v = (self.reducer)(v, value);
		self.from.insert_owned(key, v)
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
impl<Previous, Merge> Watch for Reducer<Previous, Merge>
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
