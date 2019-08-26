use adaptarr_macros::From;
use failure::Fail;
use rand::{self, Rng};
use ring::aead::{self, Aad, BoundKey, Nonce};
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

    let nonce_bytes = rand::thread_rng().gen();
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let key = aead::UnboundKey::new(&aead::AES_256_GCM, key).unwrap();
    let mut key = aead::SealingKey::new(key, SingleNonce(Some(nonce)));

    key.seal_in_place_append_tag(Aad::empty(), &mut data)
        .expect("failed to seal value");

    data.extend_from_slice(nonce_bytes.as_ref());

    Ok(data)
}

#[derive(Debug, Fail)]
pub enum SealingError {
    #[fail(display = "could not serialize: {}", _0)]
    Serialization(#[cause] rmps::encode::Error),
}

/// Decode and verify a value.
pub fn unseal<T>(key: &[u8], data: &mut [u8]) -> Result<T, UnsealingError>
where
    T: DeserializeOwned,
{
    if data.len() < 12 {
        return Err(UnsealingError::TooShort);
    }

    let index = data.len() - 12;
    let (ciphertext, nonce) = data.split_at_mut(index);
    let nonce = SingleNonce(Some(Nonce::try_assume_unique_for_key(nonce)?));

    let key = aead::UnboundKey::new(&aead::AES_256_GCM, key)?;
    let mut key = aead::OpeningKey::new(key, nonce);

    let decrypted = key.open_in_place(Aad::empty(), ciphertext)?;

    T::deserialize(&mut rmps::Deserializer::from_slice(&decrypted))
        .map_err(UnsealingError::Serialization)
}

struct SingleNonce(Option<Nonce>);

impl aead::NonceSequence for SingleNonce {
    fn advance(&mut self) -> Result<Nonce, ring::error::Unspecified> {
        self.0.take().ok_or(ring::error::Unspecified)
    }
}

#[derive(Debug, Fail, From)]
pub enum UnsealingError {
    #[fail(display = "could not deserialize: {}", _0)]
    Serialization(#[cause] #[from] rmps::decode::Error),
    #[fail(display = "could not decode: {}", _0)]
    Crypto(#[from] ring::error::Unspecified),
    #[fail(display = "not enough data to unseal")]
    TooShort,
}
