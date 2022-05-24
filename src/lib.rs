#![deny(missing_docs)]
//! Husky is a library for creating databases like iterators.
//! It is built around [sled].
//!
//! Take a look at the README to get started.
//!
//! Take a look at [ops] for a list of available operations.
//!
//! There are examples in the individual operations.
//!
//! Take a look at [traits] for a list of available traits.

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
	traits::{
		auto_inc::AutoInc, change::Change, load::Load, store::Store, view::View, watch::Watch,
	},
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
