mod store;

use anyhow::Result;
use delegate::delegate;
use std::sync::Arc;

use crate::traits::{change::Change, serial::Serial, view::View, watch::Watch};

type Transformer<K, V, NK, NV> = dyn Fn(&K, &V) -> Vec<(NK, NV)> + Send + Sync;

/// A struct that transforms entries.
/// You can create a [Transform] from a [View] struct.
///
/// [Transform] doesn't implement [View] or [Watch] unless you have the `fullscan` feature enabled.
/// Its value is a [Vec], because multiple entries can share a key.
/// # Examples
/// ```
/// # use husky::{wrappers::tree::Tree, View, Change, Operate, Load};
/// # let db = husky::open_temp().unwrap();
/// # let tree: Tree<String, u32> = db.open_tree("tree").unwrap();
/// let transform = tree
///   .transform(|k, v| vec![(*v, k.clone())])
///   .load()
///   .unwrap();
///
/// tree.insert("key", 2u32).unwrap();
///
/// let result = transform.get(2u32).unwrap();
/// assert_eq!(result, Some(vec!["key".to_string()]));
/// ```
pub struct Transform<Previous, Key, Value>
where
	Previous: View,
{
	transformer: Arc<Transformer<Previous::Key, Previous::Value, Key, Value>>,
	from: Previous,
}
impl<P, K, V> Clone for Transform<P, K, V>
where
	P: View,
{
	fn clone(&self) -> Self {
		Self {
			transformer: self.transformer.clone(),
			from: self.from.clone(),
		}
	}
}

impl<P, K, V> Transform<P, K, V>
where
	P: View + Watch,
	K: Serial,
	V: Serial,
{
	pub(crate) fn new<Transformer>(from: P, transformer: Transformer) -> Self
	where
		Transformer: 'static + Fn(&P::Key, &P::Value) -> Vec<(K, V)> + Sync + Send,
	{
		let transformer = Arc::new(transformer);
		Transform { from, transformer }
	}
}

#[cfg(feature = "fullscan")]
use std::{collections::HashMap, hash::Hash};
#[cfg(feature = "fullscan")]
impl<P, K, V> View for Transform<P, K, V>
where
	P: View,
	K: Serial + Hash + Eq,
	V: Serial,
{
	type Key = K;
	type Value = Vec<V>;
	type Iter = Box<dyn Iterator<Item = Result<(K, Vec<V>)>>>;
	fn get_ref(&self, key: &Self::Key) -> Result<Option<Self::Value>> {
		let vec = self
			.from
			.iter()
			.filter_map(|res| res.ok())
			.flat_map(|(k, v)| (self.transformer)(&k, &v))
			.filter(|(k, _)| k == key)
			.map(|(_, v)| v)
			.collect::<Vec<_>>();
		if vec.is_empty() {
			Ok(None)
		} else {
			Ok(Some(vec))
		}
	}
	fn iter(&self) -> Self::Iter {
		let transformer = Arc::clone(&self.transformer);
		let mut map = HashMap::new();
		for (k, v) in self.from.iter().flatten() {
			for (k, v) in (transformer)(&k, &v) {
				let vec = map.entry(k).or_insert_with(Vec::new);
				vec.push(v);
			}
		}
		Box::new(map.into_iter().map(Ok))
	}
}

impl<P, K, V> Change for Transform<P, K, V>
where
	P: View + Change,
	K: Serial + PartialEq,
	V: Serial,
{
	type Key = <P as Change>::Key;
	type Value = <P as Change>::Value;
	type Insert = <P as Change>::Insert;
  #[rustfmt::skip]
	delegate! {
	  to self.from {
      fn insert_owned(&self, key: Self::Key, value: Self::Insert) -> Result<Option<<Self as Change>::Value>>;
      fn insert_ref(&self,key: &<Self as Change>::Key, value: &<Self as Change>::Insert) -> Result<Option<<Self as Change>::Value>>;
      fn remove_owned(&self, key: <Self as Change>::Key) -> Result<Option<<Self as Change>::Value>>;
      fn remove_ref(&self, key: &<Self as Change>::Key) -> Result<Option<<Self as Change>::Value>>;
      fn clear(&self) -> Result<()>;
	  }
	}
}
