use std::collections::HashMap;
use std::{
	hash::Hash,
	sync::{Arc, RwLock},
};

use anyhow::Result;

use super::{change::Change, view::View};

/// Allows for loading a tree into memory. Please refer to [Loaded]
pub trait Load {
	/// The loaded type
	type Loaded;
	/// Loads the tree into memory
	fn load(&self) -> Result<Self::Loaded>;
}

/// A tree loaded in memory.
pub struct Loaded<K, V> {
	inner: Arc<RwLock<HashMap<K, V>>>,
}
impl<K, V> Loaded<K, V> {
	pub(crate) fn new() -> Self {
		Self {
			inner: Arc::default(),
		}
	}
}
impl<K, V> Clone for Loaded<K, V> {
	fn clone(&self) -> Self {
		Self {
			inner: self.inner.clone(),
		}
	}
}

impl<K, V> View for Loaded<K, V>
where
	K: 'static + Clone + Send + Sync + Hash + Eq,
	V: 'static + Clone + Send + Sync,
{
	type Key = K;
	type Value = V;
	type Iter = Box<dyn Iterator<Item = Result<(K, V)>>>;
	fn get_ref(&self, key: &Self::Key) -> Result<Option<Self::Value>> {
		let map = self.inner.read().unwrap();
		let value = map.get(key).cloned();
		Ok(value)
	}
	fn iter(&self) -> Self::Iter {
		Box::new(
			self.inner
				.read()
				.unwrap()
				.iter()
				.map(|(k, v)| Ok((k.clone(), v.clone())))
				.collect::<Vec<_>>()
				.into_iter(),
		)
	}
}

impl<K, V> Change for Loaded<K, V>
where
	K: 'static + Clone + Send + Sync + Hash + Eq,
	V: 'static + Clone + Send + Sync,
{
	type Key = K;
	type Value = V;
	type Insert = V;
	fn insert_owned(&self, key: K, value: V) -> Result<Option<<Self as Change>::Value>> {
		let mut map = self.inner.write().unwrap();
		let prev = HashMap::insert(&mut map, key, value);
		Ok(prev)
	}
	fn remove_ref(&self, key: &<Self as Change>::Key) -> Result<Option<<Self as Change>::Value>> {
		let mut map = self.inner.write().unwrap();
		let prev = HashMap::remove(&mut map, key);
		Ok(prev)
	}
	fn clear(&self) -> Result<()> {
		let mut map = self.inner.write().unwrap();
		HashMap::clear(&mut map);
		Ok(())
	}
}
