use anyhow::Result;
use delegate::delegate;
use std::marker::PhantomData;

use crate::{helpers::deserialize_option, traits::serial::Serial};

/// Wrapper around [sled::transaction::TransactionalTree]
pub struct TransactionalTree<'a, K, V> {
	inner: &'a sled::transaction::TransactionalTree,
	k: PhantomData<K>,
	v: PhantomData<V>,
}

impl<'a, K, V> From<&'a sled::transaction::TransactionalTree> for TransactionalTree<'a, K, V> {
	fn from(inner: &'a sled::transaction::TransactionalTree) -> Self {
		TransactionalTree {
			inner,
			k: PhantomData,
			v: PhantomData,
		}
	}
}

impl<'a, K, V> TransactionalTree<'a, K, V>
where
	K: Serial,
	V: Serial,
{
	/// Inserts a new key-value pair into the tree
	pub fn insert(&self, key: K, value: V) -> Result<Option<V>> {
		let key = Serial::serialize(&key)?;
		let value = Serial::serialize(&value)?;
		let value = self.inner.insert(key, value)?.map(|v| v.to_vec());
		deserialize_option(value)
	}
	/// Removes a key from the tree
	pub fn remove(&self, key: K) -> Result<Option<V>> {
		let key = Serial::serialize(&key)?;
		let value = self.inner.remove(key)?.map(|v| v.to_vec());
		deserialize_option(value)
	}
	/// Gets a value from the tree
	pub fn get(&self, key: K) -> Result<Option<V>> {
		let key = Serial::serialize(&key)?;
		let value = self.inner.get(key)?.map(|v| v.to_vec());
		deserialize_option(value)
	}
	/// Returns the inner [sled::transaction::TransactionalTree]
	pub fn to_inner(&self) -> &sled::transaction::TransactionalTree {
		self.inner
	}
	/// Returns the inner [sled::transaction::TransactionalTree]
	pub fn into_inner(self) -> &'a sled::transaction::TransactionalTree {
		self.inner
	}
  #[rustfmt::skip]
	delegate! {
	  to self.inner {
      /// Delegates to [sled::transaction::TransactionalTree::flush]
      pub fn flush(&self);
      /// Delegates to [sled::transaction::TransactionalTree::generate_id]
      pub fn generate_id(&self) -> Result<u64, sled::Error>;
	  }
	}
}
