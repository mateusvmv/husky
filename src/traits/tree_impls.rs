use anyhow::Result;
use bus::BusReader;
use delegate::delegate;
use std::{ops::RangeBounds, sync::Arc};

use crate::{
	traits::{
		change::Change,
		view::View,
		watch::{Event, Watch},
	},
	wrappers::{
		database::Db,
		tree::{self, Tree},
	},
};

use super::serial::Serial;

impl<Key, Value> View for Tree<Key, Value>
where
	Key: Serial,
	Value: Serial,
{
	type Key = Key;
	type Value = Value;
	type Iter = tree::Iter<Key, Value>;
  #[rustfmt::skip]
	delegate! {
    to self {
      fn get_ref(&self, key: &Self::Key) -> Result<Option<Self::Value>>;
      fn contains_key_ref(&self, key: &Self::Key) -> Result<bool>;
      fn get_lt_ref(&self, key: &Self::Key) -> Result<Option<(Self::Key, Self::Value)>>;
      fn get_gt_ref(&self, key: &Self::Key) -> Result<Option<(Self::Key, Self::Value)>>;
      fn first(&self) -> Result<Option<(Self::Key, Self::Value)>>;
      fn last(&self) -> Result<Option<(Self::Key, Self::Value)>>;
      fn range(&self, range: impl RangeBounds<Self::Key>) -> Result<Self::Iter>;
      fn iter(&self) -> Self::Iter;
	  }
  }
	fn is_empty(&self) -> Option<bool> {
		Some(self.is_empty())
	}
}

impl<Key, Value> Watch for Tree<Key, Value>
where
	Key: 'static + Serial + Sync + Send,
	Value: 'static + Serial + Sync + Send,
{
	fn watch(&self) -> BusReader<Event<Key, Value>> {
		self.watcher.new_reader()
	}
	fn db(&self) -> Db {
		self.db()
	}
	fn sync(&self) -> Arc<crate::threads::Synchronizer> {
		Arc::clone(&self.sync)
	}
	fn wait(&self) {
		self.sync.wait()
	}
}

impl<Key, Value> Change for Tree<Key, Value>
where
	Key: 'static + Serial + Sync + Send,
	Value: 'static + Serial + Sync + Send,
{
	type Key = Key;
	type Value = Value;
	type Insert = Value;
	fn clear(&self) -> Result<()> {
		Ok(self.clear()?)
	}
  #[rustfmt::skip]
	delegate! {
	  to self {
      fn insert_owned(&self, key: Self::Key, value: Self::Insert) -> Result<Option<<Self as Change>::Value>>;
      fn remove_owned(&self, key: <Self as Change>::Key) -> Result<Option<<Self as Change>::Value>>;
      fn fetch_and_update(
        &self,
        key: &Self::Key,
        f: impl FnMut(Option<Self::Value>) -> Option<Self::Value>,
      ) -> Result<Option<Self::Value>>;
	  }
	}
}
