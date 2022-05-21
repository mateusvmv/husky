use std::ops::RangeBounds;

use anyhow::Result;

/// Allows for viewing entries in a tree.
pub trait View
where
	Self: 'static + Clone,
{
	/// The key used on fetch.
	type Key: 'static + Clone + Send + Sync;
	/// The value expected on fetch.
	type Value: 'static + Clone + Send + Sync;
	/// The type of iterator returned by [iter](View::iter).
	type Iter: Iterator<Item = Result<(Self::Key, Self::Value)>>;
	/// Gets a value from a key by reference.
	fn get_ref(&self, key: &Self::Key) -> Result<Option<Self::Value>>;
	/// Gets a value from a key.
	fn get<K: Into<Self::Key>>(&self, key: K) -> Result<Option<Self::Value>> {
		self.get_ref(&key.into())
	}
	/// Checks if tree contains a key by reference.
	fn contains_key_ref(&self, key: &Self::Key) -> Result<bool>;
	/// Checks if tree contains a key.
	fn contains_key<K: Into<Self::Key>>(&self, key: K) -> Result<bool> {
		self.contains_key_ref(&key.into())
	}
	/// Gets the immediate lesser item by key reference.
	fn get_lt_ref(&self, key: &Self::Key) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord;
	/// Gets the immediate lesser item by key.
	fn get_lt<K: Into<Self::Key>>(&self, key: K) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		self.get_lt_ref(&key.into())
	}
	/// Gets the immediate greater item by key reference.
	fn get_gt_ref(&self, key: &Self::Key) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord;
	/// Gets the immediate greater item by key.
	fn get_gt<K: Into<Self::Key>>(&self, key: K) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		self.get_gt_ref(&key.into())
	}
	/// Gets the first item.
	fn first(&self) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord;
	/// Gets the last item.
	fn last(&self) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord;
	/// Checks if tree is empty
	fn is_empty(&self) -> bool;
	/// Gets an iterator over a key range in the tree
	fn range(&self, range: impl RangeBounds<Self::Key>) -> Result<Self::Iter>;
	/// Gets an iterator over the entries in the tree.
	fn iter(&self) -> Self::Iter;
}
