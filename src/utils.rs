use rand::{self, Rng};
use ring::aead;
use rmps;
use serde::{Serialize, de::DeserializeOwned};

/// Encrypt and sign a value.
pub fn seal<T>(key: &[u8], value: T) -> Result<Vec<u8>, SealingError>
where
    T: Serialize,
{
    let mut data = Vec::new();
    value.serialize(&mut rmps::Serializer::new(&mut data))
        .map_err(SealingError::Serialization)?;
    data.extend_from_slice(&[0; aead::MAX_TAG_LEN]);

    let key = aead::SealingKey::new(&aead::AES_256_GCM, key).unwrap();

    let nonce: [u8; 12] = rand::thread_rng().gen();

    aead::seal_in_place(&key, &nonce, &[], &mut data, aead::MAX_TAG_LEN)
        .expect("failed to seal value");

    data.extend_from_slice(&nonce);

    Ok(data)
}

/// Decode and verify a value.
pub fn unseal<T>(key: &[u8], data: &mut [u8]) -> Result<T, UnsealingError>
where
    T: DeserializeOwned,
{
    if data.len() < 12 {
        return Err(UnsealingError::TooShort);
    }

    let key = aead::OpeningKey::new(&aead::AES_256_GCM, key)?;

    let index = data.len() - 12;
    let (ciphertext, nonce) = data.split_at_mut(index);

    let decrypted = aead::open_in_place(&key, nonce, &[], 0, ciphertext)?;

    T::deserialize(&mut rmps::Deserializer::from_slice(&decrypted))
        .map_err(UnsealingError::Serialization)
}

#[derive(Debug, Fail)]
pub enum SealingError {
    #[fail(display = "could not serialize: {}", _0)]
    Serialization(#[cause] rmps::encode::Error),
}

#[derive(Debug, Fail)]
pub enum UnsealingError {
    #[fail(display = "could not deserialize: {}", _0)]
    Serialization(#[cause] rmps::decode::Error),
    #[fail(display = "could not decode: {}", _0)]
    Crypto(#[cause] ring::error::Unspecified),
    #[fail(display = "not enough data to unseal")]
    TooShort,
}

impl_from! { for UnsealingError ;
    ring::error::Unspecified => |e| UnsealingError::Crypto(e),
}
