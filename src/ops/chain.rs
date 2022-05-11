use anyhow::Result;
use bus::{Bus, BusReader};
use std::{
	hash::Hash,
	sync::{Arc, RwLock},
};

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
