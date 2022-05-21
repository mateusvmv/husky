use anyhow::Result;
use delegate::delegate;
use std::{
	collections::hash_map::DefaultHasher,
	hash::{Hash, Hasher},
};

use crate::{
	macros::hash,
	structs::single::Single,
	traits::{load::Loaded, serial::Serial},
	tree::Tree,
};

/// A wrapper around [sled::Db]
#[derive(Clone)]
pub struct Db {
	inner: sled::Db,
}

impl From<sled::Db> for Db {
	fn from(inner: sled::Db) -> Self {
		Self { inner }
	}
}

impl Db {
	/// Opens the specified tree
	pub fn open_tree<K, V, N>(&self, name: N) -> Result<Tree<K, V>>
	where
		K: Serial,
		V: Serial,
		N: Hash,
	{
		let name = hash!("tree", name);
		let inner = self.inner.open_tree(name)?;
		Ok(Tree::new(self.clone(), inner))
	}
	/// Opens a single value in the database
	pub fn open_single<K, V>(&self, key: K) -> Result<Single<V>>
	where
		K: Serial,
		V: Serial,
	{
		Single::new(self.inner.clone(), key)
	}
	/// Opens a temporary tree, loaded into memory
	pub fn open_temp<K, V>(&self) -> Loaded<K, V>
	where
		K: Serial,
		V: Serial,
	{
		Loaded::new()
	}
	/// Drops the specified tree
	pub fn drop_tree<N>(&self, name: &N) -> Result<bool>
	where
		N: Hash,
	{
		let name = hash!("tree", name);
		Ok(self.inner.drop_tree(name)?)
	}
	/// Lists all the hashed tree names
	pub fn tree_names(&self) -> Result<Vec<u64>> {
		let names = self.inner.tree_names();
		let mut deserialized = Vec::with_capacity(names.len());
		for name in names {
			let name = Serial::deserialize(name.to_vec())?;
			deserialized.push(name);
		}
		Ok(deserialized)
	}
	/// Returns the inner [sled::Db]
	pub fn to_inner(&self) -> &sled::Db {
		&self.inner
	}
	/// Returns the inner [sled::Db]
	pub fn into_inner(self) -> sled::Db {
		self.inner
	}
  #[rustfmt::skip]
	delegate! {
	  to self.inner {
      /// Delegates to [sled::Db::was_recovered]
      pub fn was_recovered(&self) -> bool;
      /// Delegated to [sled::Db::generate_id]
      pub fn generate_id(&self) -> Result<u64, sled::Error>;
      /// Delegated to [sled::Db::export]
      pub fn export(&self) -> Vec<(Vec<u8>, Vec<u8>, impl Iterator<Item = Vec<Vec<u8>>>)>;
      /// Delegated to [sled::Db::import]
      pub fn import(&self, import: Vec<(Vec<u8>, Vec<u8>, impl Iterator<Item = Vec<Vec<u8>>>)>);
      /// Delegated to [sled::Db::checksum]
      pub fn checksum(&self) -> Result<u32, sled::Error>;
      /// Delegated to [sled::Db::size_on_disk]
      pub fn size_on_disk(&self) -> Result<u64, sled::Error>;
	  }
	}
}
