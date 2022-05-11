use anyhow::Result;

use crate::{macros::unwrap_or_return, traits::serial::Serial};

pub fn deserialize_tuple<K, V>(input: Option<(Vec<u8>, Vec<u8>)>) -> Result<Option<(K, V)>>
where
	K: Serial,
	V: Serial,
{
	let (key, value) = unwrap_or_return!(input);
	let key = Serial::deserialize(key)?;
	let value = Serial::deserialize(value)?;
	Ok(Some((key, value)))
}

pub fn deserialize_option<V>(value: Option<Vec<u8>>) -> Result<Option<V>>
where
	V: Serial,
{
	let value = unwrap_or_return!(value);
	let value = Serial::deserialize(value)?;
	Ok(Some(value))
}

pub fn serialize_option<V>(value: Option<&V>) -> Result<Option<Vec<u8>>>
where
	V: Serial,
{
	let value = unwrap_or_return!(value);
	let value = Serial::serialize(value)?;
	Ok(Some(value))
}
