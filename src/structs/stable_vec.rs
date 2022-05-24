use anyhow::Result;
use bit_vec::BitVec;

use crate::traits::serial::Serial;

#[derive(Debug, Clone)]
pub struct StableVec<T> {
	vec: Vec<T>,
	used: BitVec,
}

impl<T> Default for StableVec<T> {
	fn default() -> Self {
		Self {
			vec: Default::default(),
			used: Default::default(),
		}
	}
}

impl<T> StableVec<T> {
	pub fn new() -> Self {
		Self::default()
	}
	pub fn with_capacity(capacity: usize) -> Self {
		Self {
			vec: Vec::with_capacity(capacity),
			used: BitVec::from_elem(capacity, false),
		}
	}
	pub fn push(&mut self, item: T) -> usize {
		let free = self.used.iter().position(|x| !x);
		match free {
			Some(index) => {
				self.vec[index] = item;
				self.used.set(index, true);
				index
			}
			None => {
				let i = self.vec.len();
				self.vec.push(item);
				self.used.push(true);
				i
			}
		}
	}
	pub fn extend<I>(&mut self, iter: I) -> Vec<usize>
	where
		I: IntoIterator<Item = T> + ExactSizeIterator,
	{
		let free = self
			.used
			.iter()
			.enumerate()
			.filter_map(|(i, x)| if x { None } else { Some(i) })
			.collect::<Vec<_>>();
		let to_insert = iter.len();
		let to_reserve = {
			let free_len = free.len();
			to_insert.max(free_len) - free_len
		};
		self.vec.reserve(to_reserve);
		self.used.reserve(to_reserve);

		let mut iter = iter.into_iter();
		let mut indexes = Vec::with_capacity(to_insert);
		for idx in free {
			if let Some(item) = iter.next() {
				self.used.set(idx, true);
				self.vec.insert(idx, item);
				indexes.push(idx);
			} else {
				break;
			}
		}
		for item in iter {
			let i = self.vec.len();
			self.vec.push(item);
			self.used.push(true);
			indexes.push(i);
		}
		if self.used.len() < self.vec.len() {
			eprintln!("Desync in extend");
		}
		indexes
	}
	pub fn remove(&mut self, index: usize) {
		self.used.set(index, false);
	}
	pub fn to_vec(&self) -> Vec<&T> {
		self.vec
			.iter()
			.enumerate()
			.filter_map(|(i, x)| if self.used[i] { Some(x) } else { None })
			.collect::<Vec<_>>()
	}
	pub fn into_vec(self) -> Vec<T> {
		self.vec
			.into_iter()
			.enumerate()
			.filter_map(|(i, x)| if self.used[i] { Some(x) } else { None })
			.collect::<Vec<_>>()
	}
	pub fn len(&self) -> usize {
		self.used.iter().filter(|x| *x).count()
	}
	pub fn is_empty(&self) -> bool {
		self.len() == 0
	}
}

impl<T> Serial for StableVec<T>
where
	T: Serial,
{
	fn serialize(&self) -> Result<Vec<u8>> {
		let mut items = Vec::with_capacity(self.vec.len());
		for item in self.vec.iter() {
			let item = Serial::serialize(item)?;
			items.push(item);
		}
		let free = self.used.to_bytes();
		items.push(free);
		Serial::serialize(&items)
	}

	fn deserialize(bytes: Vec<u8>) -> Result<Self> {
		let mut items: Vec<Vec<u8>> = Serial::deserialize(bytes)?;
		let free = items.pop().unwrap();
		let mut vec = Vec::with_capacity(items.len());
		for item in items.into_iter() {
			let item = Serial::deserialize(item)?;
			vec.push(item);
		}
		let free = BitVec::from_bytes(&free);
		Ok(StableVec { vec, used: free })
	}
}
