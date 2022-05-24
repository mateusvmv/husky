use anyhow::Result;
use bus::BusReader;
use delegate::delegate;
use std::{hash::Hash, ops::Deref, sync::Arc};

use crate::{
	macros::cloned,
	threads::{spawn_listener, Synchronizer},
	wrappers::{database::Db, tree::Tree},
};

use crate::traits::{
	change::Change,
	load::{Load, Loaded},
	serial::Serial,
	store::Store,
	view::View,
	watch::{Event, Watch},
};

/// A view that is stored in the database
pub struct Material<From, Inner>
where
	From: View + Watch,
	Inner: View + Change,
{
	from: From,
	inner: Inner,
	sync: Arc<Synchronizer>,
}

impl<F, I> Clone for Material<F, I>
where
	F: View + Watch,
	I: View + Change,
{
	fn clone(&self) -> Self {
		Self {
			from: self.from.clone(),
			inner: self.inner.clone(),
			sync: Arc::clone(&self.sync),
		}
	}
}

impl<From, Inner> Material<From, Inner>
where
	From: View + Watch<Key = <Inner as Change>::Key, Value = <Inner as Change>::Insert>,
	Inner: Clone + View + Change + Send + Sync,
{
	pub(crate) fn new(from: From, inner: Inner) -> Self {
		let sync = Arc::new(Synchronizer::from(vec![from.sync()]));
		spawn_listener(
			Arc::clone(&sync),
			from.watch(),
			cloned!(inner, move |event| {
				match event {
					Event::Insert { key, value } => {
						inner.insert_ref(&*key, &*value)?;
					}
					Event::Remove { key } => {
						inner.remove_ref(&*key)?;
					}
				}
				Ok(1)
			}),
		);
		Self { from, inner, sync }
	}
	/// Rebuilds the tree from its source view
	pub fn rebuild(&self) -> Result<()> {
		self.inner.clear()?;
		for res in self.from.iter() {
			let (k, v) = res?;
			self.inner.insert(k, v)?;
		}
		// The sync needs to be reset
		// For the received field to be equal to the outgoing field in the source
		// Otherwise they would never be equal, and it would wait forever on get
		self.sync.reset();
		self.from.sync().reset();
		Ok(())
	}
}

impl<From, Inner> Deref for Material<From, Inner>
where
	From: View + Watch,
	Inner: View + Change,
{
	type Target = Inner;
	fn deref(&self) -> &Self::Target {
		&self.inner
	}
}

impl<From, Inner> View for Material<From, Inner>
where
	From: View + Watch,
	Inner: View + Change,
{
	type Key = <Inner as View>::Key;
	type Value = <Inner as View>::Value;
	type Iter = Inner::Iter;
	fn get_ref(&self, key: &Self::Key) -> Result<Option<Self::Value>> {
		self.sync.wait();
		self.inner.get_ref(key)
	}
	fn iter(&self) -> Self::Iter {
		self.sync.wait();
		self.inner.iter()
	}
	fn contains_key_ref(&self, key: &Self::Key) -> Result<bool> {
		self.sync.wait();
		self.inner.contains_key_ref(key)
	}
	fn get_lt_ref(&self, key: &Self::Key) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		self.sync.wait();
		self.inner.get_lt_ref(key)
	}
	fn get_gt_ref(&self, key: &Self::Key) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		self.sync.wait();
		self.inner.get_gt_ref(key)
	}
	fn first(&self) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		self.sync.wait();
		self.inner.first()
	}
	fn last(&self) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		self.sync.wait();
		self.inner.last()
	}
	fn is_empty(&self) -> Option<bool> {
		self.sync.wait();
		self.inner.is_empty()
	}
	fn range(&self, range: impl std::ops::RangeBounds<Self::Key>) -> Result<Self::Iter> {
		self.sync.wait();
		self.inner.range(range)
	}
}
impl<From, Inner> Change for Material<From, Inner>
where
	From: View + Change + Watch,
	Inner: View + Change,
{
	type Key = <From as Change>::Key;
	type Value = <From as Change>::Value;
	type Insert = <From as Change>::Insert;
  #[rustfmt::skip]
	delegate! {
	  to self.from {
      fn insert_owned(&self, key: Self::Key, value: Self::Insert) -> Result<Option<<Self as Change>::Value>>;
      fn insert_ref(&self, key: &Self::Key, value: &Self::Insert) -> Result<Option<<Self as Change>::Value>>;
      fn remove_owned(&self, key: <Self as Change>::Key) -> Result<Option<<Self as Change>::Value>>;
      fn remove_ref(&self, key: &<Self as Change>::Key) -> Result<Option<<Self as Change>::Value>>;
      fn clear(&self) -> Result<()>;
      fn fetch_and_update(
        &self,
        key: &Self::Key,
        f: impl FnMut(Option<Self::Value>) -> Option<Self::Insert>,
      ) -> Result<Option<Self::Value>>;
	  }
	}
}
impl<From, Inner> Watch for Material<From, Inner>
where
	From: View<Key = <Inner as View>::Key, Value = <Inner as View>::Value> + Watch,
	Inner: View + Change,
{
	#[rustfmt::skip]
	delegate! {
	  to self.from {
	    fn watch(&self) -> BusReader<Event<<From as View>::Key, <From as View>::Value>>;
      fn db(&self) -> Db;
	  }
	}
	fn sync(&self) -> Arc<Synchronizer> {
		Arc::clone(&self.sync)
	}
	fn wait(&self) {
		self.sync.wait()
	}
}

impl<T> Store for T
where
	T: View + Watch,
	<T as View>::Key: Serial,
	<T as View>::Value: Serial,
{
	type Stored = Material<Self, Tree<<T as View>::Key, <T as View>::Value>>;
	fn store(&self, name: impl Hash) -> Result<Self::Stored> {
		let inner = self.db().open_tree(name)?;
		Ok(Material::new(self.clone(), inner))
	}
}

impl<T> Load for T
where
	T: View + Watch,
	<T as View>::Key: Serial + Ord,
	<T as View>::Value: Serial,
{
	type Loaded = Material<Self, Loaded<<T as View>::Key, <T as View>::Value>>;
	fn load(&self) -> Result<Self::Loaded> {
		let inner = Loaded::new();
		let res = Material::new(self.clone(), inner);
		res.rebuild()?;
		Ok(res)
	}
}
