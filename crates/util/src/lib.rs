use serde::de::{Deserialize, Deserializer};

mod secure;
mod single_init;

pub use self::{
    secure::*,
    single_init::SingleInit,
};

/// Format a byte array as a hexadecimal string.
pub fn bytes_to_hex(hash: &[u8]) -> String {
    use std::fmt::Write;

    let mut hex = String::with_capacity(hash.len() * 4);

    for byte in hash {
        write!(hex, "{:02x}", byte).unwrap();
    }

    hex
}

pub fn de_optional_null<'de, T, D>(de: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    T::deserialize(de).map(Some)
}

pub fn and_tuple<A, B>(a: Option<A>, b: Option<B>) -> Option<(A, B)> {
    match (a, b) {
        (Some(a), Some(b)) => Some((a, b)),
        _ => None,
    }
}
