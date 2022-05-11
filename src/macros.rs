macro_rules! unwrap_or_return {
	( $e:expr ) => {
		match $e {
			Some(x) => x,
			None => return Ok(None),
		}
	};
}
#[cfg(feature = "bytecheck")]
macro_rules! unwrap_or_error {
	( $e: expr ) => {
		match $e {
			Ok(x) => x,
			Err(err) => return Err(anyhow::anyhow!("{}", err)),
		}
	};
}
macro_rules! hash {
  ( $( $x:expr ),* ) => {
    {
      let mut hasher = DefaultHasher::new();
      $(
        $x.hash(&mut hasher);
      )*
      hasher.finish().to_be_bytes()
    }
  }
}
macro_rules! cloned {
  ( $a:ident, $($b:tt)+ ) => {
    {
      let $a = $a.clone();
      cloned!($($b)*)
    }
  };
  ( $f:expr ) => {
    $f
  };
}
#[cfg(feature = "bytecheck")]
pub(crate) use unwrap_or_error;
pub(crate) use {cloned, hash, unwrap_or_return};
