use adaptarr_macros::From;
use failure::Fail;
use rand::{self, Rng};
use ring::aead;
use rmps;
use serde::{Serialize, de::DeserializeOwned};
use std::{sync::atomic::{AtomicUsize, Ordering}, marker::PhantomData};

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

#[derive(Debug, Fail, From)]
pub enum UnsealingError {
    #[fail(display = "could not deserialize: {}", _0)]
    Serialization(#[cause] #[from] rmps::decode::Error),
    #[fail(display = "could not decode: {}", _0)]
    Crypto(#[cause] #[from] ring::error::Unspecified),
    #[fail(display = "not enough data to unseal")]
    TooShort,
}

/// Format a byte array as a hexadecimal string.
pub fn bytes_to_hex(hash: &[u8]) -> String {
    use std::fmt::Write;

    let mut hex = String::with_capacity(hash.len() * 4);

    for byte in hash {
        write!(hex, "{:02x}", byte).unwrap();
    }

    hex
}

/// Structure holding possibly uninitialized data.
///
/// This differs from other similar types found on crates.io in that it doesn't
/// lock or synchronise access in any way, instead assuming it is safe to
/// initialize the value multiple times, and only keep one result.
#[derive(Debug)]
pub struct SingleInit<T> {
    cell: AtomicUsize,
    _type: PhantomData<T>,
}

impl<T> SingleInit<T> {
    /// Create a new uninitialized atomic cell.
    pub const fn uninit() -> Self {
        SingleInit {
            cell: AtomicUsize::new(0),
            _type: PhantomData,
        }
    }
}

impl<T> SingleInit<T>
where
    T: Sync,
    Self: 'static,
{
    /// Get stored value, initializing it if necessary.
    pub fn get_or_init<F>(&self, init: F) -> &'static T
    where
        F: FnOnce() -> T,
    {
        let ptr = self.cell.load(Ordering::Relaxed);

        if ptr != 0 {
            return unsafe { &*(ptr as *const T) };
        }

        // Create a new value, place it on heap, obtain reference to it, and
        // prevent destructor from running.
        let value = Box::leak(Box::new(init())) as *mut T;

        // Try to update cell.
        let old = self.cell.compare_and_swap(ptr, value as usize, Ordering::Relaxed);

        if old == ptr {
            // Update succeeded, value is now the value of cell.
            unsafe { &*value }
        } else {
            // Update failed, cell was initialised by another thread. In this
            // case we drop value and return old.
            std::mem::drop(unsafe { Box::from_raw(value) });
            unsafe { &*(old as *const T) }
        }
    }

    /// Same as [`get_or_init`] except that initialisation function can fail.
    ///
    /// If initialisation function fails, the value will be unchanged and
    /// another thread (or the same thread) can safely attempt to initialise it
    /// again.
    pub fn get_or_try_init<F, E>(&self, init: F) -> Result<&'static T, E>
    where
        F: FnOnce() -> Result<T, E>,
    {
        let ptr = self.cell.load(Ordering::Relaxed);

        if ptr != 0 {
            return Ok(unsafe { &*(ptr as *const T) });
        }

        // Create a new value, place it on heap, obtain reference to it, and
        // prevent destructor from running.
        let value = Box::leak(Box::new(init()?)) as *mut T;

        // Try to update cell.
        let old = self.cell.compare_and_swap(ptr, value as usize, Ordering::Relaxed);

        if old == ptr {
            // Update succeeded, value is now the value of cell.
            Ok(unsafe { &*value })
        } else {
            // Update failed, cell was initialised by another thread. In this
            // case we drop value and return old.
            std::mem::drop(unsafe { Box::from_raw(value) });
            Ok(unsafe { &*(old as *const T) })
        }
    }
}
