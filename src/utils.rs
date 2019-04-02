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

/// See documentation for [`IteratorGroupExt::group_by_key()`].
pub trait IteratorGroupExt: Iterator {
    /// Collect sequences of elements sharing an equal key.
    ///
    /// ```ignore
    /// let a = [1, 2, 3, 4, 5, 6, 7];
    /// let b = a.into_iter()
    ///     .group_by_key::<Vec<_>, _, _>(|x| x / 2)
    ///     .collect::<Vec<_>>();
    /// assert_eq!(b, vec![vec![1], vec![2, 3], vec![4, 5], vec![6, 7]]);
    /// ```
    fn group_by_key<'c, B, F, T>(self, key: F) -> Box<dyn Iterator<Item = B> + 'c>
    where
        F: Fn(&Self::Item) -> T + 'c,
        T: Copy + Eq + 'c,
        B: Default + Extend<Self::Item> + 'c,
        Self: Sized + 'c,
    {
        Box::new(GroupByKey {
            source: self,
            acc: B::default(),
            key,
            last_key: None,
        })
    }
}

impl<T: Iterator> IteratorGroupExt for T {
}

struct GroupByKey<I, B, F, T> {
    source: I,
    acc: B,
    key: F,
    last_key: Option<T>
}

impl<I, B, F, T> Iterator for GroupByKey<I, B, F, T>
where
    I: Iterator,
    B: Default + Extend<I::Item>,
    F: Fn(&I::Item) -> T,
    T: Eq,
{
    type Item = B;

    fn next(&mut self) -> Option<B> {
        loop {
            let item = match self.source.next() {
                Some(item) => item,
                None => break,
            };
            let key = (self.key)(&item);

            if let Some(last) = self.last_key.take() {
                if last == key {
                    self.acc.extend(std::iter::once(item));
                    self.last_key = Some(last);
                } else {
                    let value = std::mem::replace(&mut self.acc, B::default());
                    self.acc.extend(std::iter::once(item));
                    self.last_key = Some(key);
                    return Some(value);
                }
            } else {
                self.acc.extend(std::iter::once(item));
                self.last_key = Some(key);
            }
        }

        if self.last_key.is_some() {
            self.last_key = None;
            Some(std::mem::replace(&mut self.acc, B::default()))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_by_key() {
        let a = vec![1, 2, 3, 4, 5, 6, 7];
        let b = a.into_iter()
            .group_by_key::<Vec<_>, _, _>(|&x| x / 2)
            .collect::<Vec<_>>();
        assert_eq!(b, vec![vec![1], vec![2, 3], vec![4, 5], vec![6, 7]]);
    }
}
