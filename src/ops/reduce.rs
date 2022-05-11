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

type Reducer<P, M> = dyn Fn(Option<<P as View>::Value>, &M) -> <P as Change>::Insert + Send + Sync;

/// A struct that reduces values on insert.
/// You can create a [Reduce] from a [Change] struct.
/// # Important
/// If you perform an insert that bypasses the [Reduce] struct, be in on the tree or in another reduce, you may experience data races.
///
/// # Examples
/// ```
/// use husky::{Tree, View, Change, Operate};
/// let db = husky::open_temp().unwrap();
/// let tree: Tree<String, String> = db.open_tree("tree").unwrap();
/// let reduce = tree.reduce(|str, insert: &String| format!("{}, {}!", str.unwrap(), insert));
///
/// tree.insert("key", "hello").unwrap();
/// reduce.insert("key", "world").unwrap();
///
/// let result = reduce.get("key").unwrap();
/// assert_eq!(result, Some("hello, world!".to_string()));
/// ```
pub struct Reduce<Previous, Merge>
where
	Previous: View + Change,
{
	reducer: Arc<Reducer<Previous, Merge>>,
	from: Previous,
}
impl<P: Clone + View + Change, M> Clone for Reduce<P, M> {
	fn clone(&self) -> Self {
		Self {
			reducer: Arc::clone(&self.reducer),
			from: self.from.clone(),
		}
	}
}

impl<P, Merge> Reduce<P, Merge>
where
	P: View + Change,
{
	pub(crate) fn new<Reducer>(from: P, reducer: Reducer) -> Self
	where
		Reducer:
			'static + Fn(Option<<P as View>::Value>, &Merge) -> <P as Change>::Insert + Send + Sync,
		P: 'static + Sync + Send,
	{
		let reducer = Arc::new(reducer);
		Reduce { from, reducer }
	}
}

impl<Previous, Merge> View for Reduce<Previous, Merge>
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
impl<Previous, Merge> Change for Reduce<Previous, Merge>
where
	Previous: View + Change<Key = <Previous as View>::Key>,
	Merge: 'static + Clone + Send + Sync,
{
	type Key = <Previous as Change>::Key;
	type Value = <Previous as Change>::Value;
	type Insert = Merge;
	fn insert_ref(
		&self,
		key: &Self::Key,
		value: &Self::Insert,
	) -> Result<Option<<Self as Change>::Value>> {
		let v = self.from.get_ref(key)?;
		let v = (self.reducer)(v, value);
		self.from.insert_owned(key.clone(), v)
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
impl<Previous, Merge> Watch for Reduce<Previous, Merge>
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
