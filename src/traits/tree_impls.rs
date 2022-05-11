use anyhow::Result;
use bus::BusReader;
use delegate::delegate;
use std::sync::Arc;

use crate::{
	traits::{
		change::Change,
		view::View,
		watch::{Event, Watch},
	},
	wrappers::{database::Db, tree::Tree},
};

use super::serial::Serial;

impl<Key, Value> View for Tree<Key, Value>
where
	Key: Serial,
	Value: Serial,
{
	type Key = Key;
	type Value = Value;
	type Iter = Box<dyn Iterator<Item = Result<(Self::Key, Self::Value)>>>;
	fn iter(&self) -> Self::Iter {
		Box::new(self.iter())
	}
	delegate! {
	  to self {
		fn get_ref(&self, key: &Self::Key) -> Result<Option<Self::Value>>;
	  }
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
	  }
	}
}
