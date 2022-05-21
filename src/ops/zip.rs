use anyhow::Result;
use bus::{Bus, BusReader};
use std::{
	cmp::Ordering,
	collections::HashMap,
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

type ZipItem<A, B> = (Option<<A as View>::Value>, Option<<B as View>::Value>);

/// A struct that zips two views together.
/// You can create a [Zip] from two [View] structs, as long as they have the same key type.
/// # Examples
/// ```
/// # use husky::{Tree, View, Change, Operate};
/// # let db = husky::open_temp().unwrap();
/// # let a_tree: Tree<String, String> = db.open_tree("a").unwrap();
/// # let b_tree: Tree<String, String> = db.open_tree("b").unwrap();
///
/// let zip = a_tree.zip(&b_tree);
///
/// a_tree.insert("key", "hello").unwrap();
/// b_tree.insert("key", "world").unwrap();
///
/// let result = zip.get("key").unwrap();
/// assert_eq!(result, Some((Some("hello".to_string()), Some("world".to_string()))));
/// ```
pub struct Zip<A, B>
where
	A: View,
	B: View<Key = A::Key>,
{
	a: A,
	b: B,
	watcher: Watcher<A::Key, ZipItem<A, B>>,
	sync: Arc<Synchronizer>,
}
impl<A, B> Clone for Zip<A, B>
where
	A: View,
	B: View<Key = A::Key>,
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

impl<A, B> Zip<A, B>
where
	A: View + Watch + Sync + Send,
	B: View<Key = <A as View>::Key> + Watch + Sync + Send,
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
				move |event| {
					let (key, a) = match event {
						Event::Insert { key, value } => (key, Some((*value).clone())),
						Event::Remove { key } => (key, None),
					};
					let b = b.get_ref(&key)?;
					let event = match (&a, &b) {
						(None, None) => Event::Remove { key },
						_ => Event::Insert {
							key,
							value: Arc::new((a, b)),
						},
					};
					Ok(vec![event])
				},
			);
			spawn_watcher(sync, b_reader, Arc::clone(&bus), move |event| {
				let (key, b) = match event {
					Event::Insert { key, value } => (key, Some((*value).clone())),
					Event::Remove { key } => (key, None),
				};
				let a = a.get_ref(&key)?;
				let event = match (&a, &b) {
					(None, None) => Event::Remove { key },
					_ => Event::Insert {
						key,
						value: Arc::new((a, b)),
					},
				};
				Ok(vec![event])
			});
			bus
		}));
		Zip {
			a,
			b,
			watcher,
			sync,
		}
	}
}

impl<A, B> View for Zip<A, B>
where
	A: View,
	B: View<Key = A::Key>,
	<A as View>::Key: Hash + Eq,
{
	type Key = A::Key;
	type Value = (Option<A::Value>, Option<B::Value>);
	type Iter = Box<dyn Iterator<Item = Result<(Self::Key, Self::Value)>>>;
	fn get_ref(&self, key: &Self::Key) -> Result<Option<Self::Value>> {
		let a = self.a.get_ref(key)?;
		let b = self.b.get_ref(key)?;
		match (&a, &b) {
			(None, None) => Ok(None),
			_ => Ok(Some((a, b))),
		}
	}
	fn iter(&self) -> Self::Iter {
		let a = self.a.iter();
		let b = self.b.iter();
		let mut map = HashMap::new();
    let mut err = Vec::new();
		a.into_iter().for_each(|r| match r {
      Ok((k, v)) => {
        let e = map.entry(k).or_insert((None, None));
        e.0 = Some(v);
      },
      Err(e) => err.push(Err(e)),
    });
		b.into_iter().for_each(|r| match r {
      Ok((k, v)) => {
        let e = map.entry(k).or_insert((None, None));
        e.1 = Some(v);
      },
      Err(e) => err.push(Err(e)),
    });
		Box::new(err.into_iter().chain(map.into_iter().map(Ok)))
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
			(Some(a), None) => {
				let (k, a) = a;
				Ok(Some((k, (Some(a), None))))
			}
			(None, Some(b)) => {
				let (k, b) = b;
				Ok(Some((k, (None, Some(b)))))
			}
			(Some(a), Some(b)) => {
				let (ka, a) = a;
				let (kb, b) = b;
				match ka.cmp(&kb) {
					Ordering::Less => Ok(Some((kb, (None, Some(b))))),
					Ordering::Equal => Ok(Some((ka, (Some(a), Some(b))))),
					Ordering::Greater => Ok(Some((ka, (Some(a), None)))),
				}
			}
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
			(Some(a), None) => {
				let (k, a) = a;
				Ok(Some((k, (Some(a), None))))
			}
			(None, Some(b)) => {
				let (k, b) = b;
				Ok(Some((k, (None, Some(b)))))
			}
			(Some(a), Some(b)) => {
				let (ka, a) = a;
				let (kb, b) = b;
				match ka.cmp(&kb) {
					Ordering::Less => Ok(Some((ka, (Some(a), None)))),
					Ordering::Equal => Ok(Some((ka, (Some(a), Some(b))))),
					Ordering::Greater => Ok(Some((kb, (None, Some(b))))),
				}
			}
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
			(None, Some(b)) => {
				let (k, b) = b;
				Ok(Some((k, (None, Some(b)))))
			}
			(Some(a), None) => {
				let (k, a) = a;
				Ok(Some((k, (Some(a), None))))
			}
			(Some(a), Some(b)) => {
				let (ka, a) = a;
				let (kb, b) = b;
				match ka.cmp(&kb) {
					Ordering::Less => Ok(Some((ka, (Some(a), None)))),
					Ordering::Equal => Ok(Some((ka, (Some(a), Some(b))))),
					Ordering::Greater => Ok(Some((kb, (None, Some(b))))),
				}
			}
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
			(None, Some(b)) => {
				let (k, b) = b;
				Ok(Some((k, (None, Some(b)))))
			}
			(Some(a), None) => {
				let (k, a) = a;
				Ok(Some((k, (Some(a), None))))
			}
			(Some(a), Some(b)) => {
				let (ka, a) = a;
				let (kb, b) = b;
				match ka.cmp(&kb) {
					Ordering::Less => Ok(Some((kb, (None, Some(b))))),
					Ordering::Equal => Ok(Some((ka, (Some(a), Some(b))))),
					Ordering::Greater => Ok(Some((ka, (Some(a), None)))),
				}
			}
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
		let mut map = HashMap::new();
    let mut err = Vec::new();
		a.for_each(|r| match r {
      Ok((k, v)) => {
        let e = map.entry(k).or_insert((None, None));
        e.0 = Some(v);
      },
      Err(e) => err.push(Err(e)),
    });
		b.for_each(|r| match r {
      Ok((k, v)) => {
        let e = map.entry(k).or_insert((None, None));
        e.1 = Some(v);
      },
      Err(e) => err.push(Err(e)),
    });
		Ok(Box::new(err.into_iter().chain(map.into_iter().map(Ok))))
	}
}

impl<A, B> Watch for Zip<A, B>
where
	A: View + Watch,
	B: View<Key = A::Key> + Watch,
	<A as View>::Key: Hash + Eq + Ord,
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
