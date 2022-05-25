use anyhow::Result;

use crate::traits::serial::Serial;

#[derive(Debug, Clone)]
pub struct StableVec<T>(Vec<Option<T>>);

impl<T> Default for StableVec<T> {
	fn default() -> Self {
		Self(Vec::new())
	}
}

impl<T> StableVec<T> {
	pub fn new() -> Self {
		Self::default()
	}
	pub fn with_capacity(capacity: usize) -> Self {
		Self(Vec::with_capacity(capacity))
	}
	pub fn push(&mut self, item: T) -> usize {
		let free = self.0.iter().position(|x| x.is_none());
		match free {
			Some(index) => {
        self.0.insert(index, Some(item));
        index
			}
			_ => {
        let i = self.0.len();
				self.0.push(Some(item));
        i
			}
		}
	}
	pub fn extend<I>(&mut self, iter: I) -> Vec<usize>
	where
		I: IntoIterator<Item = T> + ExactSizeIterator,
	{
		let free = self
			.0
			.iter()
			.enumerate()
			.filter_map(|(i, x)| if x.is_some() { None } else { Some(i) })
			.collect::<Vec<_>>();
		let to_insert = iter.len();
		let to_reserve = {
			let free_len = free.len();
			to_insert.max(free_len) - free_len
		};
		self.0.reserve(to_reserve);

		let mut iter = iter.into_iter();
		let mut indexes = Vec::with_capacity(to_insert);
		for idx in free {
			if let Some(item) = iter.next() {
				self.0.insert(idx, Some(item));
				indexes.push(idx);
			} else {
				break;
			}
		}
		for item in iter {
			let i = self.0.len();
			self.0.push(Some(item));
      indexes.push(i);
		}
		indexes
	}
	pub fn remove(&mut self, index: usize) {
		self.0.insert(index, None);
	}
	pub fn to_vec(&self) -> Vec<&T> {
		self.0
			.iter()
			.filter_map(|x| x.as_ref())
			.collect::<Vec<_>>()
	}
	pub fn into_vec(self) -> Vec<T> {
		self.0
			.into_iter()
			.filter_map(|x| x)
			.collect::<Vec<_>>()
	}
	pub fn len(&self) -> usize {
		self.0.iter().filter(|x| x.is_some()).count()
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
		let mut items = Vec::with_capacity(self.0.len());
		for item in self.0.iter() {
      let item = match item {
        Some(i) => Some(i.serialize()?),
        None => None
      };
			let item = Serial::serialize(&item)?;
			items.push(item);
		}
		Serial::serialize(&items)
	}

	fn deserialize(bytes: Vec<u8>) -> Result<Self> {
		let items: Vec<Vec<u8>> = Serial::deserialize(bytes)?;
		let mut vec = Vec::with_capacity(items.len());
		for item in items.into_iter() {
			let item: Option<Vec<u8>> = Serial::deserialize(item)?;
      let item = match item {
        Some(i) => Some(T::deserialize(i)?),
        None => None
      };
			vec.push(item);
		}
		Ok(StableVec(vec))
	}
}
