
# Husky
Husky is a library for creating databases with an API similar to iterators. Its aim is to make cache and indexing easier.

It is built around sled, and can use either rkyv or serde for serialization through feature flags.

## Features
- Typed sled trees
- Create new trees by:
	- Reindexing into multiple keys
	- Transforming into multiple entries
	- Filtering entries by key and value
	- Mapping single values
	- Chaining two trees of same type
	- Zipping two trees of same key type
- Make inserts easier by:
  - Reducing values on insert
  - Piping changes from one tree to another
- Store those trees in the database, or load them into memory

## Examples
```rust
use husky::{Tree, Operate, Change, Load, View};
// Or husky::open("db_path").unwrap()
let db = husky::open_temp().unwrap();
let tree: Tree<i32, i32> = db.open_tree("tree").unwrap();

for i in 0..100 {
  tree.insert(i, i).unwrap();
}
// Change the tree values
let double = tree.map(|_, v| v * 2);
double.iter()
  .flatten()
  .for_each(|(k, v)| assert_eq!(k * 2, v));

// Change the tree keys
let string_idx = tree.index(|k, _| vec![k.to_string()])
  .load()
  .unwrap()
  .map(|_, v| v[0]);
string_idx.iter()
  .flatten()
  .for_each(|(k, v)| assert_eq!(k, v.to_string()));

// Zip two trees
let window = tree.zip(&double);
window.iter()
  .flatten()
  .for_each(|(k, (v, d))| {
    assert_eq!(Some(k), v);
    assert_eq!(Some(k * 2), d);
  });
```