use anyhow::Result;
use std::marker::PhantomData;

use crate::{helpers::deserialize_option, traits::serial::Serial};

/// Represents an entry in the database's top level tree
/// Can be used for singletons
pub struct Single<V>
where
	V: Serial,
{
	db: sled::Db,
	key: Vec<u8>,
	v: PhantomData<V>,
}
impl<V> Single<V>
where
	V: Serial,
{
	/// Creates a new singleton in a database from a key
	/// Represents an entry in that database
	pub fn new<K>(db: sled::Db, key: K) -> Result<Self>
	where
		K: Serial,
	{
		let key = key.serialize()?;
		Ok(Self {
			db,
			key,
			v: PhantomData,
		})
	}
	/// Loads the value from the entry
	pub fn get(&self) -> Result<Option<V>> {
		let value = self.db.get(&self.key)?;
		deserialize_option(value.map(|v| v.to_vec()))
	}
	/// Inserts an owned value into the entry
	pub fn insert_owned(&self, value: V) -> Result<Option<V>> {
		let value = value.serialize()?;
		let old_value = self.db.insert(self.key.clone(), value)?;
		deserialize_option(old_value.map(|v| v.to_vec()))
	}
	/// Inserts a borrowed value into the entry
	pub fn insert_ref(&self, value: &V) -> Result<Option<V>> {
		let value = value.serialize()?;
		let old_value = self.db.insert(self.key.clone(), value)?;
		deserialize_option(old_value.map(|v| v.to_vec()))
	}
	/// Inserts something that can be converted into a value into the entry
	pub fn insert<IV>(&self, value: IV) -> Result<Option<V>>
	where
		IV: Into<V>,
	{
		let value = value.into();
		self.insert_owned(value)
	}
}
