use std::marker::PhantomData;

use anyhow::Result;

/// An iterator over a tree
pub struct Iter<F, O, R>
where
	O: Fn(F::Item) -> Result<R>,
	F: Iterator,
{
	from: F,
	operation: O,
	r: PhantomData<R>,
}

impl<F, O, R> Iter<F, O, R>
where
	O: Fn(F::Item) -> Result<R>,
	F: Iterator,
{
	pub fn new(from: F, operation: O) -> Self {
		Self {
			from,
			operation,
			r: PhantomData,
		}
	}
}

impl<F, O, R> Iterator for Iter<F, O, R>
where
	O: Fn(F::Item) -> Result<R>,
	F: Iterator,
{
	type Item = Result<R>;

	fn next(&mut self) -> Option<Self::Item> {
		self.from.next().map(|item| (self.operation)(item))
	}
}
