#![deny(missing_docs)]
//! Husky is a library for creating databases like iterators.
//! It is built around [sled].
//!
//! For operations, refer to [ops]. There are more examples there, too.
//!
//! For others, take a look at [traits]
//! # Examples
//! ```
//! use husky::{Tree, Operate, Change, Load, View};
//! // Or husky::open("db_path").unwrap()
//! let db = husky::open_temp().unwrap();
//! let tree: Tree<i32, i32> = db.open_tree("tree").unwrap();
//!
//! for i in 0..100 {
//!   tree.insert(i, i).unwrap();
//! }
//! // Change the tree values
//! let double = tree.map(|_, v| v * 2);
//! double.iter()
//!   .flatten()
//!   .for_each(|(k, v)| assert_eq!(k * 2, v));
//!
//! // Change the tree keys
//! let string_idx = tree.index(|k, _| vec![k.to_string()])
//!   .load()
//!   .unwrap()
//!   .map(|_, v| v[0]);
//! string_idx.iter()
//!   .flatten()
//!   .for_each(|(k, v)| assert_eq!(k, v.to_string()));
//!
//! // Zip two trees
//! let window = tree.zip(&double);
//! window.iter()
//!   .flatten()
//!   .for_each(|(k, (v, d))| {
//!     assert_eq!(Some(k), v);
//!     assert_eq!(Some(k * 2), d);
//!   });
//! ```

use anyhow::Result;
use std::path::Path;

mod helpers;
mod macros;
/// Various operations for transforming trees
pub mod ops;
mod structs;
mod threads;
/// Traits for viewing, watching and changing trees
pub mod traits;
/// Wrappers around sled
pub mod wrappers;

pub use {
	ops::Operate,
	structs::{material::Material, single::Single},
	traits::{change::Change, load::Load, store::Store, view::View, watch::Watch},
	wrappers::{batch::Batch, tree::Tree},
};

pub use database::Db;
use wrappers::*;

/// Opens a database at the given path
pub fn open(path: impl AsRef<Path>) -> Result<Db> {
	let db = sled::open(path)?;
	Ok(Db::from(db))
}

/// Opens a database in memory
pub fn open_temp() -> Result<Db> {
	let db = sled::Config::new().temporary(true).open()?;
	Ok(Db::from(db))
}

#[cfg(test)]
mod tests;
