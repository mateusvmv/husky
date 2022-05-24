/// An auto-incrementable key.
pub trait AutoInc {
	/// The next item in the sequence.
	fn next(&self) -> Self;
	/// The first item in the sequence.
	fn first() -> Self;
}

macro_rules! impl_auto_inc {
	($t:ty) => {
		impl AutoInc for $t {
			fn next(&self) -> Self {
				*self + 1
			}
			fn first() -> Self {
				1
			}
		}
	};
}

impl_auto_inc!(u8);
impl_auto_inc!(u16);
impl_auto_inc!(u32);
impl_auto_inc!(u64);
impl_auto_inc!(u128);
impl_auto_inc!(usize);
