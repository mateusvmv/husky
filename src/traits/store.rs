use std::hash::Hash;

use anyhow::Result;

/// Represents a struct that can be stored in a tree.
pub trait Store {
	/// The stored type
	type Stored;
	/// Stores the struct
	fn store(&self, name: impl Hash) -> Result<Self::Stored>;
}
