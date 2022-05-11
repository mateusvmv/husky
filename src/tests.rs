use crate::{
	database::Db,
	ops::Operate,
	traits::{change::Change, load::Load, serial::Serial, store::Store, view::View},
	tree::Tree,
};

fn with_tree<K, V>(f: impl FnOnce(Tree<K, V>))
where
	K: Serial,
	V: Serial,
{
	with_db(|db: Db| {
		let tree = db.open_tree("tree").expect("Failed to open test tree");
		f(tree);
	});
}

fn with_db(f: impl FnOnce(Db)) {
	let config = sled::Config::new().temporary(true);
	let db = config.open().expect("Failed to open test db");
	let db = Db::from(db);
	f(db);
}

#[test]
fn tree_name_persists() {
	with_db(|db: Db| {
		let tree: Tree<u32, u32> = db.open_tree("tree").expect("Failed to open tree");
		for i in 0u32..10 {
			tree.insert(i, i).expect("Failed to insert");
		}
		drop(tree);
		let tree: Tree<u32, u32> = db.open_tree("tree").expect("Failed to open tree");
		for i in 0..10 {
			assert_eq!(tree.get(i).expect("Failed to get tree value"), Some(i));
		}
	})
}

#[test]
fn serial() {
	let value = "hello".to_string();
	let serial = Serial::serialize(&value).unwrap();
	let deserial: String = Serial::deserialize(serial).unwrap();
	assert_eq!(value, deserial);
}

const TEST_SIZE: u32 = 40;
fn insert<C: Change<Key = u32, Value = u32, Insert = u32>>(tree: &C, pow: u32) {
	for i in 0..TEST_SIZE {
		tree.insert(i, i.pow(pow)).unwrap();
	}
}

fn assert_u32<V: View<Key = u32, Value = u32>>(tree: &V, pow: u32) {
	for i in 0..TEST_SIZE {
		assert_eq!(tree.get(i).unwrap(), Some(i.pow(pow)));
	}
}

fn assert_none<V: View<Key = u32>>(tree: &V) {
	for i in 0..TEST_SIZE {
		assert!(tree.get(i).unwrap().is_none());
	}
}

fn remove<C: Change<Key = u32, Value = u32>>(tree: &C) {
	for i in 0..TEST_SIZE {
		tree.remove(i).unwrap();
	}
}

fn assert_reindex<V: View<Key = u32, Value = u32>>(tree: &V, pow: u32) {
	for i in 0..TEST_SIZE {
		assert_eq!(tree.get(i * 3).unwrap(), Some(i.pow(pow)));
	}
}

#[test]
fn transform() {
	with_tree(|tree: Tree<u32, u32>| {
		let transform = tree.transform(|k, v| vec![(*k, v * v)]);
		let stored = transform
			.store("stored_transform")
			.expect("Failed to store transform")
			.map(|_, v| v[0]);
		let loaded = transform.load().unwrap().map(|_, v| v[0]);

		insert(&stored, 2);

		assert_u32(&stored, 4);
		assert_u32(&loaded, 4);

		remove(&stored);

		#[cfg(feature = "fullscan")]
		assert_none(&transform);
		assert_none(&stored);
		assert_none(&loaded);
	})
}

#[test]
fn transform_rebuild() {
	with_tree(|tree: Tree<u32, u32>| {
		let transform = tree.transform(|k, v| vec![(*k, v * v)]);

		insert(&tree, 2);

		let stored = transform
			.store("stored_transform")
			.expect("Failed to store transform");
		let mapped = stored.map(|_, v| v[0]);

		assert_none(&mapped);
		stored.rebuild().expect("Rebuild failed");
		assert_u32(&mapped, 4);
	});
}

#[test]
fn transform_replaces() {
	with_tree(|tree: Tree<u32, u32>| {
		let transform = tree.transform(|k, v| vec![(*k, v * v)]);
		let stored = transform
			.store("stored_transform")
			.expect("Failed to store transform")
			.map(|_, v| v[0]);
		let loaded = transform.load().unwrap().map(|_, v| v[0]);

		insert(&tree, 2);

		assert_u32(&stored, 4);
		assert_u32(&loaded, 4);

		insert(&tree, 3);

		assert_u32(&stored, 6);
		assert_u32(&loaded, 6);
	})
}

#[test]
fn transform_reindex() {
	with_tree(|tree: Tree<u32, u32>| {
		let transform = tree.transform(|k, v| vec![(*k * 3, v * v)]);
		let stored = transform
			.store("stored_transform")
			.expect("Failed to store transform")
			.map(|_, v| v[0]);
		let loaded = transform.load().unwrap().map(|_, v| v[0]);

		insert(&tree, 2);

		assert_reindex(&stored, 4);
		assert_reindex(&loaded, 4);
	})
}

#[test]
fn map() {
	with_tree(|tree: Tree<u32, u32>| {
		let mapped = tree.map(|_, v| v * v);
		let stored = mapped.store("stored_map").expect("Failed to store map");
		let loaded = mapped.load().unwrap();

		insert(&tree, 2);

		assert_u32(&mapped, 4);
		assert_u32(&stored, 4);
		assert_u32(&loaded, 4);

		remove(&tree);

		assert_none(&mapped);
		assert_none(&stored);
		assert_none(&loaded);
	});
}

#[test]
fn map_replaces() {
	with_tree(|tree: Tree<u32, u32>| {
		let mapped = tree.map(|_, v| v * v);
		let stored = mapped.store("stored_map").expect("Failed to store map");
		let loaded = mapped.load().unwrap();

		insert(&tree, 2);

		assert_u32(&mapped, 4);
		assert_u32(&stored, 4);
		assert_u32(&loaded, 4);

		insert(&tree, 3);

		assert_u32(&mapped, 6);
		assert_u32(&stored, 6);
		assert_u32(&loaded, 6);
	});
}

#[test]
fn map_rebuild() {
	with_tree(|tree: Tree<u32, u32>| {
		let mapped = tree.map(|_, v| v * v);

		insert(&tree, 2);
		assert_u32(&mapped, 4);

		let loaded = mapped.load().unwrap();
		let stored = mapped.store("stored_map").unwrap();
		assert_u32(&loaded, 4);
		assert_none(&stored);

		loaded.rebuild().expect("Rebuild failed");
		stored.rebuild().expect("Rebuild failed");

		assert_u32(&stored, 4);
		assert_u32(&loaded, 4);
	});
}

#[test]
fn chain() {
	with_db(|db| {
		let a: Tree<u32, u32> = db.open_tree("a").unwrap();
		let b: Tree<u32, u32> = db.open_tree("b").unwrap();
		let chained = a.chain(&b);
		let stored = chained
			.store("stored_chain")
			.expect("Failed to store chain");
		let loaded = chained.load().unwrap();

		insert(&a, 2);

		assert_u32(&chained, 2);
		assert_u32(&stored, 2);
		assert_u32(&loaded, 2);

		remove(&a);

		assert_none(&chained);
		assert_none(&stored);
		assert_none(&loaded);

		insert(&b, 3);

		assert_u32(&chained, 3);
		assert_u32(&stored, 3);
		assert_u32(&loaded, 3);

		insert(&a, 4);

		assert_u32(&chained, 4);
		assert_u32(&stored, 4);
		assert_u32(&loaded, 4);
	});
}

#[test]
fn zip() {
	with_db(|db| {
		let a: Tree<u32, u32> = db.open_tree("a").unwrap();
		let b: Tree<u32, u32> = db.open_tree("b").unwrap();
		let zipped = a.zip(&b);
		let stored = zipped.store("stored_zip").expect("Failed to store zip");
		let loaded = zipped.load().unwrap();

		insert(&a, 2);

		for i in 0..TEST_SIZE {
			let v = zipped.get(i).unwrap();
			assert_eq!(v, Some((Some(i.pow(2)), None)));
			let v = stored.get(i).unwrap();
			assert_eq!(v, Some((Some(i.pow(2)), None)));
			let v = loaded.get(i).unwrap();
			assert_eq!(v, Some((Some(i.pow(2)), None)));
		}

		remove(&a);

		assert_none(&zipped);
		assert_none(&stored);
		assert_none(&loaded);

		insert(&b, 3);

		for i in 0..TEST_SIZE {
			let v = zipped.get(i).unwrap();
			assert_eq!(v, Some((None, Some(i.pow(3)))));
			let v = stored.get(i).unwrap();
			assert_eq!(v, Some((None, Some(i.pow(3)))));
			let v = loaded.get(i).unwrap();
			assert_eq!(v, Some((None, Some(i.pow(3)))));
		}

		insert(&a, 4);

		for i in 0..TEST_SIZE {
			let v = zipped.get(i).unwrap();
			assert_eq!(v, Some((Some(i.pow(4)), Some(i.pow(3)))));
			let v = stored.get(i).unwrap();
			assert_eq!(v, Some((Some(i.pow(4)), Some(i.pow(3)))));
			let v = loaded.get(i).unwrap();
			assert_eq!(v, Some((Some(i.pow(4)), Some(i.pow(3)))));
		}
	});
}

#[test]
fn filter() {
	with_tree(|tree: Tree<u32, u32>| {
		let filtered = tree.filter(|_, v| v % 2 == 0);
		let stored = filtered
			.store("stored_filter")
			.expect("Failed to store filter");
		let loaded = filtered.load().unwrap();

		insert(&tree, 2);

		for i in 0..TEST_SIZE {
			let expected = if i % 2 == 0 { Some(i.pow(2)) } else { None };
			let v = filtered.get(i).unwrap();
			assert_eq!(v, expected);
			let v = stored.get(i).unwrap();
			assert_eq!(v, expected);
			let v = loaded.get(i).unwrap();
			assert_eq!(v, expected);
		}

		insert(&tree, 3);

		for i in 0..TEST_SIZE {
			let expected = if i % 2 == 0 { Some(i.pow(3)) } else { None };
			let v = filtered.get(i).unwrap();
			assert_eq!(v, expected);
			let v = stored.get(i).unwrap();
			assert_eq!(v, expected);
			let v = loaded.get(i).unwrap();
			assert_eq!(v, expected);
		}

		remove(&tree);

		assert_none(&filtered);
		assert_none(&stored);
		assert_none(&loaded);
	});
}

#[test]
fn filter_map() {
	with_tree(|tree: Tree<u32, u32>| {
		let filtered = tree.filter_map(|_, v| if v % 2 == 0 { Some(*v) } else { None });
		let stored = filtered
			.store("stored_filter")
			.expect("Failed to store filter");
		let loaded = filtered.load().unwrap();

		insert(&tree, 2);

		for i in 0..TEST_SIZE {
			let expected = if i % 2 == 0 { Some(i.pow(2)) } else { None };
			let v = filtered.get(i).unwrap();
			assert_eq!(v, expected);
			let v = stored.get(i).unwrap();
			assert_eq!(v, expected);
			let v = loaded.get(i).unwrap();
			assert_eq!(v, expected);
		}

		insert(&tree, 3);

		for i in 0..TEST_SIZE {
			let expected = if i % 2 == 0 { Some(i.pow(3)) } else { None };
			let v = filtered.get(i).unwrap();
			assert_eq!(v, expected);
			let v = stored.get(i).unwrap();
			assert_eq!(v, expected);
			let v = loaded.get(i).unwrap();
			assert_eq!(v, expected);
		}

		remove(&tree);

		assert_none(&filtered);
		assert_none(&stored);
		assert_none(&loaded);
	});
}

#[test]
fn reduce() {
	with_tree(|tree: Tree<u32, u32>| {
		let reducer = tree.reduce(|a, b| a.unwrap_or(0) + b);
		let stored = reducer
			.store("stored_reduce")
			.expect("Failed to store reduce");
		let loaded = reducer.load().unwrap();

		insert(&reducer, 2);

		for i in 0..TEST_SIZE {
			let expected = i.pow(2);
			let v = reducer.get(i).unwrap();
			assert_eq!(v, Some(expected));
			let v = stored.get(i).unwrap();
			assert_eq!(v, Some(expected));
			let v = loaded.get(i).unwrap();
			assert_eq!(v, Some(expected));
		}

		insert(&reducer, 3);

		for i in 0..TEST_SIZE {
			let expected = i.pow(2) + i.pow(3);
			let v = reducer.get(i).unwrap();
			assert_eq!(v, Some(expected));
			let v = stored.get(i).unwrap();
			assert_eq!(v, Some(expected));
			let v = loaded.get(i).unwrap();
			assert_eq!(v, Some(expected));
		}

		remove(&reducer);

		assert_none(&reducer);
		assert_none(&stored);
		assert_none(&loaded);
	});
}
