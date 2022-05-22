mod store;

use anyhow::Result;
use delegate::delegate;
use std::sync::Arc;

use crate::traits::{change::Change, serial::Serial, view::View, watch::Watch};

type Indexer<K, V, I> = dyn Fn(&K, &V) -> Vec<I> + Send + Sync;

/// A struct that reindexes entries.
/// You can create an [Index] from a [View] struct.
///
/// [Index] doesn't implement [View] or [Watch], you must store it first.
/// Its value is a [Vec], because multiple entries can share a key.
/// # Examples
/// ```
/// # use husky::{wrappers::tree::Tree, View, Change, Operate, Load};
/// # let db = husky::open_temp().unwrap();
/// # let tree: Tree<String, u32> = db.open_tree("tree").unwrap();
/// let index = tree
///   .index(|k, v| vec!["new_key".to_string()])
///   .load()
///   .unwrap();
///
/// tree.insert("key", 2u32).unwrap();
///
/// let result = index.get("new_key").unwrap();
/// assert_eq!(result, Some(vec![2u32]));
/// ```
pub struct Index<Previous, IndexKey>
where
	Previous: View,
{
	indexer: Arc<Indexer<Previous::Key, Previous::Value, IndexKey>>,
	from: Previous,
}
impl<P, I> Clone for Index<P, I>
where
	P: View,
{
	fn clone(&self) -> Self {
		Self {
			indexer: self.indexer.clone(),
			from: self.from.clone(),
		}
	}
}

impl<P, I> Index<P, I>
where
	P: View + Watch,
	I: Serial,
{
	pub(crate) fn new<Indexer>(from: P, indexer: Indexer) -> Self
	where
		Indexer: 'static + Fn(&P::Key, &P::Value) -> Vec<I> + Sync + Send,
	{
		let indexer = Arc::new(indexer);
		Index { from, indexer }
	}
}

impl<P, I> Change for Index<P, I>
where
	P: View + Change,
	I: Serial + PartialEq,
{
	type Key = <P as Change>::Key;
	type Value = <P as Change>::Value;
	type Insert = <P as Change>::Insert;
  #[rustfmt::skip]
	delegate! {
	  to self.from {
      fn insert_owned(&self, key: Self::Key, value: Self::Insert) -> Result<Option<<Self as Change>::Value>>;
      fn insert_ref(&self, key: &<Self as Change>::Key, value: &<Self as Change>::Insert) -> Result<Option<<Self as Change>::Value>>;
      fn remove_owned(&self, key: <Self as Change>::Key) -> Result<Option<<Self as Change>::Value>>;
      fn remove_ref(&self, key: &<Self as Change>::Key) -> Result<Option<<Self as Change>::Value>>;
      fn clear(&self) -> Result<()>;
	  }
	}
}
