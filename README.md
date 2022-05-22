

# Husky
Husky is an abstraction around sled that allows for the creation of views with an API similar to iterators.
Its aim is to make cache and indexing easier.

Below is a list of features. For examples, check the documentation.

## Sections
- [Add Dependency](#add-dependency)
- [Open Database](#open-a-database)
- [Open Tree](#open-a-tree)
- [View](#viewing)
  - [Is Empty](#check-if-a-view-is-empty)
  - [Key Exists](#check-if-a-key-exists)
  - [Get](#get-individual-values)
  - [Lesser and Greater](#get-entries-before-and-after)
  - [Range](#get-a-range-of-entries)
  - [Iter](#get-all-the-entries)
  - [First and Last](#get-the-first-and-last-entries)
- [Change](#changing)
  - [Insert](#insert-an-entry)
  - [Remove](#remove-an-entry)
  - [Clear](#clear-all-entries)
  - [Auto Increment](#insert-with-auto-increment)
- [Operate](#operating)
  - [Map](#map-entries)
  - [Transform](#transform-entries)
  - [Index](#reindex-entries)
  - [Chain](#chain-two-views)
  - [Zip](#zip-two-views)
  - [Filter](#filter-entries)
  - [Reducer](#reduce-inserts)
  - [Inserter](#parse-inserts)
  - [Pipe](#pipe-changes-to-another-tree)
- [Store and Load](#storing)
- [Watch](#listening)


## Getting Started
### Add Dependency
To use husky with rkyv
```toml
husky = "0.2"
```
To use husky with serde
```toml
husky = { version = "0.2", default-features = false, features = ["serde"] }
```

### Open a Database
Open a database with
```rust
let db = husky::open("db_name").unwrap();
```
or
```rust
let db = husky::open_temp().unwrap();
```

### Open a Tree
You can open a single entry in the database
```rust
let single = db.open_single("key").unwrap();
```
A key-value tree on disk
```rust
let tree = db.open_tree("name").unwrap();
```
Or a temporary key-value tree
```rust
let temp = db.open_temp();
```

### Viewing
Through the View trait you can query entries in the tree.
```rust
use husky::View;
```
#### Check if a view is empty
```rust
assert_eq!(tree.is_empty(), Some(false));
```
#### Check if a key exists
```rust
assert_eq!(tree.contains_key(1),  Ok(true));
assert_eq!(tree.contains_key(2),  Ok(true));
```
#### Get individual values
```rust
assert_eq!(tree.get(1),  Ok(Some("first value")));
assert_eq!(tree.get(2),  Ok(Some("last  value")));
```
#### Get entries before and after
```rust
assert_eq!(tree.get_lt(2), Ok(Some("first value"));
assert_eq!(tree.get_gt(1), Ok(Some("last  value"));
```
#### Get a range of entries
```rust
let mut range = tree.range(..).unwrap();
assert_eq!(range.next(),  Ok(Some((1, "first value"))));
assert_eq!(range.next(),  Ok(Some((2, "last  value"))));
```
#### Get all the entries
```rust
let mut iter = tree.iter();
assert_eq!(iter.next(),  Ok(Some((1, "first value"))));
assert_eq!(iter.next(),  Ok(Some((2, "last  value"))));
```
#### Get the first and last entries
```rust
assert_eq!(tree.first(),  Ok(Some((1, "first value"))));
assert_eq!(tree.last() ,  Ok(Some((2, "last  value"))));
```

### Changing
Through the Change trait you can manipulate the entries in the tree
```rust
use husky::Change;
```
#### Insert an entry
```rust
let previous = tree.insert("key", "value").unwrap();
```
#### Remove an entry
```rust
let previous = tree.remove("key").unwrap();
```
#### Clear all entries
```rust
tree.clear().unwrap();
```
#### Insert with auto increment
If the key type has the AutoInc trait implemented, you can push values.
By default it is implemented for all unsigned integers and usize.
```rust
tree.push("value").unwrap()
```

### Operating
Through the Operate trait you can create new views.
They are lazy wrappers around the original, but you can store their results.
```rust
use husky::Operate;
```
#### Map entries
```rust
let map = tree.map(|key, value| "new_value");
```
#### Transform entries
```rust
let transform = tree.map(|key, value| vec![
  ("first  key", "first  value"),
  ("second key", "second value")
]);
```
#### Reindex entries
```rust
let index = tree.map(|key, value| vec![
  "first  key",
  "second key"
]);
```
#### Chain two views
```rust
let chain = tree.chain(&other_tree);
```
#### Zip two views
```rust
let zip = tree.zip(&other_tree);
let (a, b) = zip.unzip();
```
#### Filter entries
```rust
let filter = tree.filter(|key, value| false);
let filter = tree.filter_map(|key, value| Some(value));
```
#### Reduce inserts
```rust
let reducer = tree.reducer(|value, add| value.unwrap_or(0) + add);
```
#### Filter and reduce inserts
```rust
let reducer = tree.filter_reducer(|value, add| value.map(|v| v + add));
```
#### Parse inserts
```rust
let inserter = tree.inserter(|insert| insert);
```
#### Filter and parse inserts
```rust
let inserter = tree.filter_inserter(|insert| Some(insert));
```
#### Pipe changes to another tree
```rust
tree.pipe(&other_tree);
```

Note that transform and index will also change the value type to a vector, because overwrites can happen.
To further operate a transform or index, you must store or load them, as they require a key map.

### Storing
You can store a view on the database through the Store trait
```rust
use husky::Store;
let stored = tree.store("tree name").unwrap();
```
Or load it in memory through the Load trait
```rust
use husky::Load;
let loaded = tree.load().unwrap();
```
Once you load or store a tree its results will be cached, and it will spawn new threads on each operation to propagate events from the original tree.

### Listening
The Watch trait provides you with access to a BusReader that listens to events in a view.
```rust
use husky::Watch.
let reader = tree.watch();
```
A function to get the original tree's database.
```rust
let db = tree.db();
```
And methods to synchronize changes.
```rust
let sync = tree.sync();
assert_eq!(sync.incoming(), 0);
assert_eq!(sync.is_sync(), true);
sync.wait();
tree.wait();
```