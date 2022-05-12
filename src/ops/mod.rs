use crate::{
	threads::spawn_listener,
	traits::{
		change::Change,
		serial::Serial,
		view::View,
		watch::{Event, Watch},
	},
};

use self::{
	chain::Chain, filter::Filter, filter_map::FilterMap, index::Index, map::Map, reducer::Reducer,
	transform::Transform, zip::Zip,
};

/// [Chain] struct declaration and implementations.
pub mod chain;
/// [Filter] struct declaration and implementations.
pub mod filter;
/// [FilterMap] struct declaration and implementations.
pub mod filter_map;
/// [Index] struct declaration and implementations.
pub mod index;
/// [Map] struct declaration and implementations.
pub mod map;
/// [Reduce] struct declaration and implementations.
pub mod reducer;
/// [Transform] struct declaration and implementations.
pub mod transform;
/// [Zip] struct declaration and implementations.
pub mod zip;

/// A trait that allows you to operate trees.
pub trait Operate
where
	Self: Sized + Clone + View + Watch + Sync + Send,
{
	/// Changes entry values. Please refer to [Map]
	fn map<M, Mapped>(&self, mapper: M) -> Map<Self, Mapped>
	where
		M: 'static + Fn(&Self::Key, &Self::Value) -> Mapped + Sync + Send,
		Mapped: 'static + Clone + Send + Sync,
	{
		Map::new(self.clone(), mapper)
	}
	/// Transforms an entry into multiple entries. Please refer to [Transform]
	fn transform<K, V, T>(&self, transformer: T) -> Transform<Self, K, V>
	where
		T: 'static + Fn(&Self::Key, &Self::Value) -> Vec<(K, V)> + Sync + Send,
		K: Serial,
		V: Serial,
	{
		Transform::new(self.clone(), transformer)
	}
	/// Changes entry keys. Please refer to [Index]
	fn index<F, I>(&self, indexer: F) -> Index<Self, I>
	where
		F: 'static + Fn(&Self::Key, &Self::Value) -> Vec<I> + Sync + Send,
		I: Serial,
	{
		Index::new(self.clone(), indexer)
	}
	/// Chains two trees together. Please refer to [Chain]
	fn chain<B>(&self, other: &B) -> Chain<Self, B>
	where
		Self: Sync + Send,
		B: View<Key = Self::Key, Value = Self::Value> + Watch + Sync + Send,
	{
		Chain::new(self.clone(), other.clone())
	}
	/// Zips two trees together. Please refer to [Zip]
	fn zip<B>(&self, other: &B) -> Zip<Self, B>
	where
		Self: Sync + Send,
		B: View<Key = Self::Key> + Watch + Sync + Send,
	{
		Zip::new(self.clone(), other.clone())
	}
	/// Creates two new trees from a tuple tree, essentially undoing [Zip].
	fn unzip<A, B>(&self) -> (Map<Self, A>, Map<Self, B>)
	where
		Self: View<Value = (A, B)>,
		A: Serial,
		B: Serial,
	{
		let a = self.map(|_, (a, _)| a.clone());
		let b = self.map(|_, (_, b)| b.clone());
		(a, b)
	}
	/// Filters values in a tree. Please refer to [Filter]
	fn filter<F>(&self, filter: F) -> Filter<Self>
	where
		F: 'static + Fn(&Self::Key, &Self::Value) -> bool + Sync + Send,
	{
		Filter::new(self.clone(), filter)
	}
	/// Filters values in a tree after a map. Please refer to [FilterMap]
	fn filter_map<F, Mapped>(&self, mapper: F) -> FilterMap<Self, Mapped>
	where
		F: 'static + Fn(&Self::Key, &Self::Value) -> Option<Mapped> + Sync + Send,
		Mapped: 'static + Clone + Send + Sync,
	{
		FilterMap::new(self.clone(), mapper)
	}
	/// Reduces inserts to a tree. Please refer to [Reducer]
	fn reducer<ReduceFn, Merge>(&self, reducer: ReduceFn) -> Reducer<Self, Merge>
	where
		Self: Change,
		ReduceFn: 'static
			+ Fn(Option<<Self as View>::Value>, &Merge) -> <Self as Change>::Insert
			+ Sync
			+ Send,
	{
		Reducer::new(self.clone(), reducer)
	}
	/// Pipes changes to another tree.
	fn pipe<O>(&self, other: O)
	where
		O: Change<Key = Self::Key, Insert = Self::Value> + Watch + Send + Sync,
	{
		let sync = other.sync();
		sync.push_source(self.sync());
		spawn_listener(sync, self.watch(), move |event| {
			let (key, value) = match event {
				Event::Insert { key, value } => (key, Some(value)),
				Event::Remove { key } => (key, None),
			};
			match value {
				Some(value) => other.insert_ref(&*key, &*value)?,
				None => other.remove_ref(&*key)?,
			};
			// No outgoing events, because the calls to insert and remove will create events already.
			Ok(0)
		});
	}
}

impl<T> Operate for T where Self: Clone + Sized + View + Watch + Sync + Send {}
