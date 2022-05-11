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
	/// Gets a value from a key reference.
	fn get_ref(&self, key: &Self::Key) -> Result<Option<Self::Value>>;
	/// Gets a value from a key.
	fn get<K: Into<Self::Key>>(&self, key: K) -> Result<Option<Self::Value>> {
		self.get_ref(&key.into())
	}
	/// Gets an iterator over the entries in the tree.
	fn iter(&self) -> Self::Iter;
}
