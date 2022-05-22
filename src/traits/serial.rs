use anyhow::Result;

/// Represents values that can be transformed into bytes.
pub trait Serial
where
	Self: 'static + Sized + Clone + Sync + Send,
{
	/// Converts the value into bytes.
	fn serialize(&self) -> Result<Vec<u8>>;
	/// Recovers the value from bytes.
	fn deserialize(bytes: Vec<u8>) -> Result<Self>;
}

#[cfg(all(not(feature = "rkyv"), not(feature = "serde")))]
compile_error!("You must specify a serializer, either rkyv or serde");
#[cfg(all(not(feature = "rkyv"), feature = "bytecheck"))]
compile_error!("bytecheck must be used with rkyv");
#[cfg(all(feature = "rkyv", feature = "serde"))]
compile_error!(
	"Can't serialize using rkyv and serde at the same time. \
                Try disabling default features in the CLI with the '--no-default-features' flag \
                or in the Cargo.toml with 'husky = { default-features = false }'"
);

#[cfg(feature = "rkyv")]
mod rkyv {
	#[cfg(feature = "bytecheck")]
	mod checked {
		use crate::{macros::unwrap_or_error, traits::serial::Serial};
		use anyhow::Result;
		use bytecheck::CheckBytes;
		use rkyv::{
			ser::serializers::AllocSerializer, validation::validators::DefaultValidator, Archive,
			Deserialize, Infallible, Serialize,
		};
		impl<T> Serial for T
		where
			T: 'static + Sized + Clone + Archive + Serialize<AllocSerializer<256>> + Sync + Send,
			<T as Archive>::Archived:
				for<'a> CheckBytes<DefaultValidator<'a>> + Deserialize<T, Infallible>,
		{
			fn serialize(&self) -> Result<Vec<u8>> {
				let serialized = rkyv::to_bytes::<_, 256>(self)?;
				Ok(serialized.to_vec())
			}
			fn deserialize(bytes: Vec<u8>) -> Result<Self> {
				let archived = rkyv::check_archived_root::<T>(&bytes);
				let archived = unwrap_or_error!(archived);
				let deserialized = archived.deserialize(&mut Infallible).unwrap();
				Ok(deserialized)
			}
		}
	}
	#[cfg(not(feature = "bytecheck"))]
	mod unchecked {
		use crate::traits::serial::Serial;
		use anyhow::Result;
		use rkyv::{
			ser::serializers::AllocSerializer, Archive, Deserialize, Infallible, Serialize,
		};
		impl<T> Serial for T
		where
			T: 'static + Sized + Clone + Archive + Serialize<AllocSerializer<256>> + Sync + Send,
			<T as Archive>::Archived: Deserialize<T, Infallible>,
		{
			fn serialize(&self) -> Result<Vec<u8>> {
				let serialized = rkyv::to_bytes::<_, 256>(self)?;
				Ok(serialized.to_vec())
			}
			fn deserialize(bytes: Vec<u8>) -> Result<Self> {
				let archived = unsafe { rkyv::archived_root::<T>(&bytes) };
				let deserialized = archived.deserialize(&mut Infallible).unwrap();
				Ok(deserialized)
			}
		}
	}
}
#[cfg(all(feature = "serde", not(feature = "rkyv")))]
mod serde {
	use crate::traits::serial::Serial;
	use anyhow::Result;
	use bincode::{DefaultOptions, Options, config::{WithOtherEndian, BigEndian}};
	use serde::{Deserialize, Serialize};
  fn big_endian() -> WithOtherEndian<DefaultOptions, BigEndian> {
    DefaultOptions::new().with_big_endian()
  }
	impl<T> Serial for T
	where
		T: 'static + Sized + Clone + Serialize + for<'a> Deserialize<'a> + Sync + Send,
	{
		fn serialize(&self) -> Result<Vec<u8>> {
      Ok(big_endian().serialize(&self)?)
		}
		fn deserialize(bytes: Vec<u8>) -> Result<Self> {
			Ok(big_endian().deserialize(&bytes)?)
		}
	}
}
