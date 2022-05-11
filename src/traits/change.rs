use anyhow::Result;

use super::view::View;

enum EntryKey<'a, K> {
	Ref(&'a K),
	Owned(K),
}

/// An entry in a tree
pub struct Entry<'a, F>
where
	F: View
		+ Change<Key = <F as View>::Key, Value = <F as View>::Value, Insert = <F as View>::Value>,
{
	key: EntryKey<'a, <F as View>::Key>,
	value: Option<<F as View>::Value>,
	from: &'a F,
}
impl<'a, F> Entry<'a, F>
where
	F: View
		+ Change<Key = <F as View>::Key, Value = <F as View>::Value, Insert = <F as View>::Value>,
{
	/// Inserts to entry, if empty
	pub fn or_insert_with(
		&mut self,
		f: impl FnOnce() -> <F as View>::Value,
	) -> &mut <F as View>::Value {
		if let Some(ref mut value) = self.value {
			value
		} else {
			self.value = Some(f());
			self.value.as_mut().unwrap()
		}
	}
	/// Removes value from entry
	pub fn remove(&mut self) -> Result<Option<<F as View>::Value>> {
		self.value.take();
		let k = match &self.key {
			EntryKey::Ref(k) => k,
			EntryKey::Owned(k) => k,
		};
		self.from.remove_ref(k)
	}
}
impl<'a, F> Drop for Entry<'a, F>
where
	F: View
		+ Change<Key = <F as View>::Key, Value = <F as View>::Value, Insert = <F as View>::Value>,
{
	fn drop(&mut self) {
		if let Some(value) = &self.value {
			let key = match &self.key {
				EntryKey::Ref(k) => k,
				EntryKey::Owned(k) => k,
			};
			self.from.insert_ref(key, value).unwrap();
		}
	}
}

/// Allows for changes to trees.
pub trait Change
where
	Self: Sized,
{
	/// The type of key that is used on inserts and removals.
	type Key: 'static + Clone + Send + Sync;
	/// The type of value expected on removal.
	type Value: 'static + Clone + Send + Sync;
	/// The type of value expected on inserts.
	type Insert: 'static + Clone + Send + Sync;
	/// Inserts an owned key-value pair into the tree.
	fn insert_owned(
		&self,
		key: Self::Key,
		value: Self::Insert,
	) -> Result<Option<<Self as Change>::Value>> {
		self.insert_ref(&key, &value)
	}
	/// Inserts a reference key-value pair into the tree.
	fn insert_ref(
		&self,
		key: &Self::Key,
		value: &Self::Insert,
	) -> Result<Option<<Self as Change>::Value>> {
		self.insert(key.clone(), value.clone())
	}
	/// Inserts a key-value pair into the tree.
	fn insert<IK: Into<Self::Key>, IV: Into<Self::Insert>>(
		&self,
		key: IK,
		value: IV,
	) -> Result<Option<<Self as Change>::Value>> {
		self.insert_owned(key.into(), value.into())
	}
	/// Gets an [Entry] from a key reference.
	fn entry_ref<'a>(&'a self, key: &'a <Self as Change>::Key) -> Result<Entry<'a, Self>>
	where
		Self: View
			+ Change<
				Key = <Self as View>::Key,
				Value = <Self as View>::Value,
				Insert = <Self as View>::Value,
			>,
	{
		let value = self.get_ref(key)?;
		Ok(Entry {
			key: EntryKey::Ref(key),
			value,
			from: self,
		})
	}
	/// Gets an [Entry] from a key.
	fn entry<K: Into<<Self as Change>::Key>>(&self, key: K) -> Result<Entry<'_, Self>>
	where
		Self: View
			+ Change<
				Key = <Self as View>::Key,
				Value = <Self as View>::Value,
				Insert = <Self as View>::Value,
			>,
	{
		let key = key.into();
		let value = self.get_ref(&key)?;
		Ok(Entry {
			key: EntryKey::Owned(key),
			value,
			from: self,
		})
	}
	/// Removes an owned key.
	fn remove_owned(&self, key: <Self as Change>::Key) -> Result<Option<<Self as Change>::Value>> {
		self.remove_ref(&key)
	}
	/// Removes a key reference.
	fn remove_ref(&self, key: &<Self as Change>::Key) -> Result<Option<<Self as Change>::Value>> {
		self.remove_owned(key.clone())
	}
	/// Removes a key.
	fn remove<K: Into<Self::Key>>(&self, key: K) -> Result<Option<<Self as Change>::Value>> {
		let key = key.into();
		self.remove_owned(key)
	}
	/// Clears the tree.
	fn clear(&self) -> Result<()>;
}
