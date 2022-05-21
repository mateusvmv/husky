use anyhow::Result;
use bus::Bus;
use delegate::delegate;
use sled::IVec;
use std::{
	ops::{Bound, RangeBounds},
	sync::Arc,
};
use parking_lot::RwLock;

use crate::{
	batch::Batch,
	database::Db,
	helpers::{deserialize_option, deserialize_tuple, serialize_option},
	macros::unwrap_or_return,
	structs::iter,
	threads::Synchronizer,
	traits::{
		serial::Serial,
		watch::{Event, Watcher},
	},
	transaction::TransactionalTree,
};

pub(crate) type Iter<K, V> =
	iter::Iter<sled::Iter, fn(Result<(IVec, IVec), sled::Error>) -> Result<(K, V)>, (K, V)>;

/// Wrapper around [sled::Tree]
pub struct Tree<K, V>
where
	K: Serial,
	V: Serial,
{
	db: Db,
	inner: sled::Tree,
	pub(crate) watcher: Arc<Watcher<K, V>>,
	pub(crate) sync: Arc<Synchronizer>,
}

impl<K, V> Clone for Tree<K, V>
where
	K: Serial,
	V: Serial,
{
	fn clone(&self) -> Self {
		Self {
			db: self.db.clone(),
			inner: self.inner.clone(),
			watcher: self.watcher.clone(),
			sync: Arc::clone(&self.sync),
		}
	}
}

impl<K, V> Tree<K, V>
where
	K: Serial,
	V: Serial,
{
	/// Gets the database that stores this tree
	pub fn db(&self) -> Db {
		self.db.clone()
	}
	pub(crate) fn new(db: Db, inner: sled::Tree) -> Self {
		let sync = Arc::new(Synchronizer::new());
		let watcher = Watcher::new(move || Arc::new(RwLock::new(Bus::new(128))));
		let watcher = Arc::new(watcher);
		Tree {
			db,
			inner,
			watcher,
			sync,
		}
	}
	/// Inserts a owned key-value pair into the tree
	/// Please refer to [Change](crate::Change)
	pub fn insert_owned(&self, key: K, value: V) -> Result<Option<V>> {
		self.sync.outgoing(1);
		let old_value = {
			let key = Serial::serialize(&key)?;
			let value = Serial::serialize(&value)?;
			self.inner.insert(key, value)?
		};
		let key = Arc::new(key);
		let value = Arc::new(value);
		self.watcher.send(Event::Insert { key, value });
		let old_value = unwrap_or_return!(old_value);
		let old_value = Serial::deserialize(old_value.to_vec())?;
		Ok(Some(old_value))
	}
	/// Opens a [TransactionalTree](crate::transaction::TransactionalTree)
	pub fn transaction<F, R, E>(&self, f: F) -> sled::transaction::TransactionResult<R, E>
	where
		F: Fn(&TransactionalTree<K, V>) -> sled::transaction::ConflictableTransactionResult<R, E>,
	{
		self.inner
			.transaction(|t: &sled::transaction::TransactionalTree| {
				let tree = TransactionalTree::from(t);
				f(&tree)
			})
	}
	/// Applies a [Batch](crate::Batch) to the tree
	pub fn apply_batch(&self, batch: Batch<K, V>) -> Result<(), sled::Error> {
		self.inner.apply_batch(batch.into())
	}
	/// Gets the value for a given key reference
	/// Please refer to [View](crate::View)
	pub fn get_ref(&self, key: &K) -> Result<Option<V>> {
		self.sync.wait();
		let key = Serial::serialize(key)?;
		let value = self.inner.get(&key)?.map(|v| v.to_vec());
		deserialize_option(value)
	}
	/// Removes a owned key
	/// Please refer to [Change](crate::Change)
	pub fn remove_owned(&self, key: K) -> Result<Option<V>> {
		self.sync.outgoing(1);
		let ser_key = Serial::serialize(&key)?;

		let key = Arc::new(key);
		self.watcher.send(Event::Remove { key });

		let value = self.inner.remove(&ser_key)?.map(|v| v.to_vec());
		deserialize_option(value)
	}
	/// Delegates to [sled::Tree::compare_and_swap]
	pub fn compare_and_swap(&self, key: &K, old: Option<&V>, new: Option<&V>) -> Result<()> {
		let key = Serial::serialize(key)?;
		let old = serialize_option(old)?;
		let new = serialize_option(new)?;
		self.inner.compare_and_swap(key, old, new)??;
		Ok(())
	}
	/// Delegates to [sled::Tree::update_and_fetch]
	pub fn update_and_fetch(
		&self,
		key: &K,
		mut f: impl FnMut(Option<V>) -> Option<V>,
	) -> Result<Option<V>> {
		let key = Serial::serialize(key)?;
		let value = self
			.inner
			.update_and_fetch(key, |v| {
				let value = v.and_then(|v| Serial::deserialize(v.into()).ok());
				let value = f(value);
				value.and_then(|value| Serial::serialize(&value).ok())
			})?
			.map(|v| v.to_vec());
		deserialize_option(value)
	}
	/// Delegates to [sled::Tree::fetch_and_update]
	pub fn fetch_and_update(
		&self,
		key: &K,
		mut f: impl FnMut(Option<V>) -> Option<V>,
	) -> Result<Option<V>> {
		let key = Serial::serialize(key)?;
		let value = self
			.inner
			.fetch_and_update(key, |v| {
				let value = v.and_then(|v| Serial::deserialize(v.into()).ok());
				let value = f(value);
				value.and_then(|value| Serial::serialize(&value).ok())
			})?
			.map(|v| v.to_vec());
		deserialize_option(value)
	}
	/// Delegates to [sled::Tree::contains_key]
	pub fn contains_key_ref(&self, key: &K) -> Result<bool> {
		let key = Serial::serialize(key)?;
		Ok(self.inner.contains_key(&key)?)
	}
	/// Delegates to [sled::Tree::get_lt]
	pub fn get_lt_ref(&self, key: &K) -> Result<Option<(K, V)>> {
		let key = Serial::serialize(key)?;
		deserialize_tuple(
			self.inner
				.get_lt(&key)?
				.map(|(k, v)| (k.to_vec(), v.to_vec())),
		)
	}
	/// Delegates to [sled::Tree::get_gt]
	pub fn get_gt_ref(&self, key: &K) -> Result<Option<(K, V)>> {
		let key = Serial::serialize(key)?;
		deserialize_tuple(
			self.inner
				.get_gt(&key)?
				.map(|(k, v)| (k.to_vec(), v.to_vec())),
		)
	}
	/// Delegates to [sled::Tree::first]
	pub fn first(&self) -> Result<Option<(K, V)>> {
		deserialize_tuple(self.inner.first()?.map(|(k, v)| (k.to_vec(), v.to_vec())))
	}
	/// Delegates to [sled::Tree::last]
	pub fn last(&self) -> Result<Option<(K, V)>> {
		deserialize_tuple(self.inner.last()?.map(|(k, v)| (k.to_vec(), v.to_vec())))
	}
	/// Delegates to [sled::Tree::pop_max]
	pub fn pop_max(&self) -> Result<Option<(K, V)>> {
		deserialize_tuple(self.inner.pop_max()?.map(|(k, v)| (k.to_vec(), v.to_vec())))
	}
	/// Delegates to [sled::Tree::pop_min]
	pub fn pop_min(&self) -> Result<Option<(K, V)>> {
		deserialize_tuple(self.inner.pop_min()?.map(|(k, v)| (k.to_vec(), v.to_vec())))
	}
	/// Delegates to [sled::Tree::iter]
	pub fn iter(&self) -> Iter<K, V> {
		Iter::new(self.inner.iter(), deserialize_entry)
	}
	/// Returns a range over the entries in the tree.
	pub fn range(&self, range: impl RangeBounds<K>) -> Result<Iter<K, V>> {
		let from = match range.start_bound() {
			Bound::Included(i) => Bound::Included(Serial::serialize(i)?),
			Bound::Excluded(i) => Bound::Excluded(Serial::serialize(i)?),
			Bound::Unbounded => Bound::Unbounded,
		};
		let to = match range.end_bound() {
			Bound::Included(i) => Bound::Included(Serial::serialize(i)?),
			Bound::Excluded(i) => Bound::Excluded(Serial::serialize(i)?),
			Bound::Unbounded => Bound::Unbounded,
		};
		let range = self.inner.range((from, to));
		Ok(Iter::new(range, deserialize_entry))
	}
	/// Returns the inner [sled::Tree]
	pub fn to_inner(&self) -> &sled::Tree {
		&self.inner
	}
	/// Returns the inner [sled::Tree]
	pub fn into_inner(self) -> sled::Tree {
		self.inner
	}
  #[rustfmt::skip]
	delegate! {
	  to self.inner {
      /// Delegates to [sled::Tree::flush]
      pub fn flush(&self) -> Result<usize, sled::Error>;
      /// Delegates to [sled::Tree::flush_async]
      pub async fn flush_async(&self) -> Result<usize, sled::Error>;
      /// Delegates to [sled::Tree::len]
      pub fn len(&self) -> usize;
      /// Delegates to [sled::Tree::is_empty]
      pub fn is_empty(&self) -> bool;
      /// Delegates to [sled::Tree::clear]
      pub fn clear(&self) -> Result<(), sled::Error>;
      /// Delegates to [sled::Tree::name]
      pub fn name(&self) -> IVec;
      /// Delegates to [sled::Tree::checksum]
      pub fn checksum(&self) -> Result<u32, sled::Error>;
	  }
	}
}

fn deserialize_entry<K, V>(r: Result<(IVec, IVec), sled::Error>) -> Result<(K, V)>
where
	K: Serial,
	V: Serial,
{
	let (key, value) = r?;
	let key = Serial::deserialize(key.to_vec())?;
	let value = Serial::deserialize(value.to_vec())?;
	Ok((key, value))
}
