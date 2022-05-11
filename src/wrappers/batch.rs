use anyhow::Result;
use std::marker::PhantomData;

use crate::traits::serial::Serial;

/// A wrapper around [sled::Batch]
#[derive(Default)]
pub struct Batch<K, V> {
	inner: sled::Batch,
	k: PhantomData<K>,
	v: PhantomData<V>,
}

impl<K, V> Batch<K, V>
where
	K: Serial,
	V: Serial,
{
	/// Insert a new key-value pair into the batch
	/// # Examples
	/// ```
	/// use husky::Batch;
	/// let mut batch: Batch<String, String> = Batch::default();
	/// batch.insert("key", "value");
	/// ```
	pub fn insert<IK: Into<K>, IV: Into<V>>(&mut self, key: IK, value: IV) -> Result<()> {
		let key = key.into();
		let value = value.into();
		let key = Serial::serialize(&key)?;
		let value = Serial::serialize(&value)?;
		self.inner.insert(key, value);
		Ok(())
	}
	/// Remove a key from the batch
	/// # Examples
	/// ```
	/// use husky::Batch;
	/// let mut batch: Batch<String, String> = Batch::default();
	/// batch.insert("key", "value");
	/// batch.remove("key");
	pub fn remove<RK: Into<K>>(&mut self, key: RK) -> Result<()> {
		let key = key.into();
		let key = Serial::serialize(&key)?;
		self.inner.remove(key);
		Ok(())
	}
}

impl<K, V> From<Batch<K, V>> for sled::Batch {
	fn from(batch: Batch<K, V>) -> Self {
		batch.inner
	}
}
