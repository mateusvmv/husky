use anyhow::Result;
use bus::{Bus, BusReader};
use std::{
	cmp::Ordering,
	hash::Hash,
	sync::Arc,
};
use parking_lot::RwLock;

use crate::{
	macros::cloned,
	threads::{spawn_watcher, Synchronizer},
	traits::{
		view::View,
		watch::{Event, Watch, Watcher},
	},
};

/// A struct that chains two trees.
/// You can create a [Chain] from two [View] structs, as long as they have the same keys and values.
/// It gives preference to the first tree.
/// # Examples
/// ```
/// # use husky::{Tree, Change, View, Operate};
/// # let db = husky::open_temp().unwrap();
/// # let a_tree: Tree<String, String> = db.open_tree("a").unwrap();
/// # let b_tree: Tree<String, String> = db.open_tree("b").unwrap();
///
/// let chain = a_tree.chain(&b_tree);
///
/// b_tree.insert("key", "b").unwrap();
/// let result = chain.get("key").unwrap();
/// assert_eq!(result, Some("b".to_string()));
///
/// a_tree.insert("key", "a").unwrap();
/// let result = chain.get("key").unwrap();
/// assert_eq!(result, Some("a".to_string()));
/// ```
pub struct Chain<A, B>
where
	A: View,
	B: View<Key = A::Key, Value = A::Value>,
{
	a: A,
	b: B,
	watcher: Watcher<A::Key, A::Value>,
	sync: Arc<Synchronizer>,
}
impl<A, B> Clone for Chain<A, B>
where
	A: View,
	B: View<Key = A::Key, Value = A::Value>,
{
	fn clone(&self) -> Self {
		Self {
			a: self.a.clone(),
			b: self.b.clone(),
			watcher: self.watcher.clone(),
			sync: Arc::clone(&self.sync),
		}
	}
}

impl<A, B> Chain<A, B>
where
	A: View + Watch + Sync + Send,
	B: View<Key = <A as View>::Key, Value = <A as View>::Value> + Watch + Sync + Send,
{
	pub(crate) fn new(a: A, b: B) -> Self {
		let sync = Arc::new(Synchronizer::from(vec![a.sync(), b.sync()]));
		let watcher = Watcher::new(cloned!(sync, a, b, move || {
			let bus = Arc::new(RwLock::new(Bus::new(128)));
			let a_reader = a.watch();
			let b_reader = b.watch();
			spawn_watcher(
				Arc::clone(&sync),
				a_reader,
				Arc::clone(&bus),
				cloned!(move |event| {
					let (key, value) = match event {
						Event::Insert { key, value } => {
							(Arc::clone(&key), Some(Arc::clone(&value)))
						}
						Event::Remove { key } => {
							(Arc::clone(&key), b.get_ref(&*key)?.map(Arc::new))
						}
					};
					let event = match value {
						Some(value) => Event::Insert { key, value },
						None => Event::Remove { key },
					};
					Ok(vec![event])
				}),
			);
			spawn_watcher(
				sync,
				b_reader,
				Arc::clone(&bus),
				cloned!(move |event| {
					let (key, value) = match event {
						Event::Insert { key, value } => {
							(Arc::clone(&key), Some(Arc::clone(&value)))
						}
						Event::Remove { key } => {
							(Arc::clone(&key), a.get_ref(&*key)?.map(Arc::new))
						}
					};
					let event = match value {
						Some(value) => Event::Insert { key, value },
						None => Event::Remove { key },
					};
					Ok(vec![event])
				}),
			);
			bus
		}));
		Chain {
			a,
			b,
			watcher,
			sync,
		}
	}
}

impl<A, B> View for Chain<A, B>
where
	A: View,
	B: View<Key = A::Key, Value = A::Value>,
{
	type Key = A::Key;
	type Value = A::Value;
	type Iter = std::iter::Chain<A::Iter, B::Iter>;
	fn get_ref(&self, key: &Self::Key) -> Result<Option<Self::Value>> {
		let a = self.a.get_ref(key)?;
		if let Some(a) = a {
			return Ok(Some(a));
		}
		let b = self.b.get_ref(key)?;
		Ok(b)
	}
	fn iter(&self) -> Self::Iter {
		let a = self.a.iter();
		let b = self.b.iter();
		a.chain(b)
	}
	fn contains_key_ref(&self, key: &Self::Key) -> Result<bool> {
		Ok(self.a.contains_key_ref(key)? || self.b.contains_key_ref(key)?)
	}
	fn get_lt_ref(&self, key: &Self::Key) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		let a = self.a.get_lt_ref(key)?;
		let b = self.b.get_lt_ref(key)?;
		match (a, b) {
			(None, None) => Ok(None),
			(Some(a), None) => Ok(Some(a)),
			(None, Some(b)) => Ok(Some(b)),
			(Some(a), Some(b)) => match a.0.cmp(&b.0) {
				Ordering::Less => Ok(Some(b)),
				Ordering::Equal => Ok(Some(a)),
				Ordering::Greater => Ok(Some(a)),
			},
		}
	}
	fn get_gt_ref(&self, key: &Self::Key) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		let a = self.a.get_gt_ref(key)?;
		let b = self.b.get_gt_ref(key)?;
		match (a, b) {
			(None, None) => Ok(None),
			(Some(a), None) => Ok(Some(a)),
			(None, Some(b)) => Ok(Some(b)),
			(Some(a), Some(b)) => match a.0.cmp(&b.0) {
				Ordering::Less => Ok(Some(a)),
				Ordering::Equal => Ok(Some(a)),
				Ordering::Greater => Ok(Some(b)),
			},
		}
	}
	fn first(&self) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		let a = self.a.first()?;
		let b = self.b.first()?;
		match (a, b) {
			(None, None) => Ok(None),
			(Some(a), None) => Ok(Some(a)),
			(None, Some(b)) => Ok(Some(b)),
			(Some(a), Some(b)) => match a.0.cmp(&b.0) {
				Ordering::Less => Ok(Some(a)),
				Ordering::Equal => Ok(Some(a)),
				Ordering::Greater => Ok(Some(b)),
			},
		}
	}
	fn last(&self) -> Result<Option<(Self::Key, Self::Value)>>
	where
		Self::Key: Ord,
	{
		let a = self.a.last()?;
		let b = self.b.last()?;
		match (a, b) {
			(None, None) => Ok(None),
			(Some(a), None) => Ok(Some(a)),
			(None, Some(b)) => Ok(Some(b)),
			(Some(a), Some(b)) => match a.0.cmp(&b.0) {
				Ordering::Less => Ok(Some(b)),
				Ordering::Equal => Ok(Some(a)),
				Ordering::Greater => Ok(Some(a)),
			},
		}
	}
	fn is_empty(&self) -> bool {
		self.a.is_empty() && self.b.is_empty()
	}
	fn range(&self, range: impl std::ops::RangeBounds<Self::Key>) -> Result<Self::Iter> {
		let a = (range.start_bound(), range.end_bound());
		let b = (range.start_bound(), range.end_bound());
		let a = self.a.range(a)?;
		let b = self.b.range(b)?;
		Ok(a.chain(b))
	}
}

impl<A, B> Watch for Chain<A, B>
where
	A: View + Watch,
	B: View<Key = A::Key, Value = A::Value> + Watch,
	<A as View>::Key: Hash + Eq,
{
	fn watch(&self) -> BusReader<Event<Self::Key, Self::Value>> {
		self.watcher.new_reader()
	}
	fn db(&self) -> crate::wrappers::database::Db {
		self.a.db()
	}
	fn sync(&self) -> Arc<Synchronizer> {
		Arc::clone(&self.sync)
	}
	fn wait(&self) {
		self.a.wait();
		self.b.wait();
	}
}
