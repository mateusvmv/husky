use std::collections::BTreeMap;
use std::sync::Arc;
use parking_lot::RwLock;

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
	inner: Arc<RwLock<BTreeMap<K, V>>>,
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
	K: 'static + Clone + Send + Sync + Ord,
	V: 'static + Clone + Send + Sync,
{
	type Key = K;
	type Value = V;
	type Iter = Box<dyn Iterator<Item = Result<(K, V)>>>;
	fn get_ref(&self, key: &Self::Key) -> Result<Option<Self::Value>> {
		let map = self.inner.read();
		let value = map.get(key).cloned();
		Ok(value)
	}
	fn iter(&self) -> Self::Iter {
		Box::new(
			self.inner
				.read()
				.iter()
				.map(|(k, v)| Ok((k.clone(), v.clone())))
				.collect::<Vec<_>>()
				.into_iter(),
		)
	}
	fn contains_key_ref(&self, key: &Self::Key) -> Result<bool> {
		Ok(self.inner.read().contains_key(key))
	}
	fn get_lt_ref(&self, key: &Self::Key) -> Result<Option<(Self::Key, Self::Value)>> {
		let map = self.inner.read();
		let value = map.range(..key).next_back();
		Ok(value.map(|(k, v)| (k.clone(), v.clone())))
	}
	fn get_gt_ref(&self, key: &Self::Key) -> Result<Option<(Self::Key, Self::Value)>> {
		let map = self.inner.read();
		let value = map.range(key..).next();
		Ok(value.map(|(k, v)| (k.clone(), v.clone())))
	}
	fn first(&self) -> Result<Option<(Self::Key, Self::Value)>> {
		let map = self.inner.read();
		let value = map.range(..).next();
		Ok(value.map(|(k, v)| (k.clone(), v.clone())))
	}
	fn last(&self) -> Result<Option<(Self::Key, Self::Value)>> {
		let map = self.inner.read();
		let value = map.range(..).next_back();
		Ok(value.map(|(k, v)| (k.clone(), v.clone())))
	}
	fn is_empty(&self) -> bool {
		self.inner.read().is_empty()
	}
	fn range(&self, range: impl std::ops::RangeBounds<Self::Key>) -> Result<Self::Iter> {
		Ok(Box::new(
			Arc::clone(&self.inner)
				.read()
				.range(range)
				.map(|(k, v)| Ok((k.clone(), v.clone())))
				.collect::<Vec<_>>()
				.into_iter()
				.collect::<Vec<_>>()
				.into_iter(),
		))
	}
}

impl<K, V> Change for Loaded<K, V>
where
	K: 'static + Clone + Send + Sync + Ord,
	V: 'static + Clone + Send + Sync,
{
	type Key = K;
	type Value = V;
	type Insert = V;
	fn insert_owned(&self, key: K, value: V) -> Result<Option<<Self as Change>::Value>> {
		let mut map = self.inner.write();
		let prev = BTreeMap::insert(&mut map, key, value);
		Ok(prev)
	}
	fn remove_ref(&self, key: &<Self as Change>::Key) -> Result<Option<<Self as Change>::Value>> {
		let mut map = self.inner.write();
		let prev = BTreeMap::remove(&mut map, key);
		Ok(prev)
	}
	fn clear(&self) -> Result<()> {
		let mut map = self.inner.write();
		BTreeMap::clear(&mut map);
		Ok(())
	}
}
