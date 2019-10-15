use bytes::{Buf, BufMut, Bytes};

pub trait BufExt: Buf {
    fn get_leb128(&mut self) -> u64;
}

impl<T: Buf> BufExt for T {
    fn get_leb128(&mut self) -> u64 {
        let mut buf = [0; 1];
        let mut v = 0u64;

        for shift in (0..).step_by(7) {
            self.copy_to_slice(&mut buf);

            let byte = u64::from(buf[0] & 0x7f)
                .checked_shl(shift)
                .expect("overflow");
            v = v.checked_add(byte)
                .expect("overflow");

            if buf[0] & 0x80 == 0 {
                break
            }
        }

        v
    }
}

pub trait BufMutExt: BufMut {
fn put_leb128(&mut self, v: u64);
}

impl<T: BufMut> BufMutExt for T {
    fn put_leb128(&mut self, mut v: u64) {
        while v >= 0x80 {
            let b = (v & 0x7f) as u8 | 0x80;
            self.put_slice(&[b]);
            v >>= 7;
        }

        self.put_slice(&[(v & 0xff) as u8]);
    }
}

pub struct ReadBytes<'bytes> {
    bytes: &'bytes Bytes,
    cursor: usize,
    limit: usize,
}

impl<'bytes> ReadBytes<'bytes> {
    pub fn new(bytes: &'bytes Bytes) -> Self {
        ReadBytes {
            bytes,
            cursor: 0,
            limit: bytes.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.limit >= self.cursor
    }

    pub fn get_slice(&mut self, len: usize) -> &'bytes [u8] {
        assert!(len <= self.remaining());
        let slice = &self.bytes.as_ref()[..len];
        self.advance(len);
        slice
    }

    pub fn slice(&mut self, len: usize) -> ReadBytes<'bytes> {
        let slice = ReadBytes {
            bytes: self.bytes,
            cursor: self.cursor,
            limit: len,
        };
        self.advance(len);
        slice
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// This is the same as `AsRef::as_ref`, except that the returned slice has
    /// lifetime of `'bytes` and is not tied to this instance.
    pub fn as_slice(&self) -> &'bytes [u8] {
        self.bytes.as_ref()
    }

    pub fn as_bytes(&self) -> Bytes {
        self.bytes.slice(self.cursor, self.limit)
    }
}

impl<'bytes> AsRef<[u8]> for ReadBytes<'bytes> {
    fn as_ref(&self) -> &[u8] {
        self.bytes.as_ref()
    }
}

impl<'bytes> Buf for ReadBytes<'bytes> {
    fn remaining(&self) -> usize {
        self.limit - self.cursor
    }

    fn bytes(&self) -> &[u8] {
        &self.bytes.as_ref()[self.cursor..]
    }

    fn advance(&mut self, cnt: usize) {
        self.cursor += cnt;
    }
}

#[cfg(test)]
mod tests {
    use bytes::*;

    use super::*;

    #[test]
    fn decode() {
        let mut b = Bytes::from_static(b"\x02\x7f\x80\x01\x81\x01\x82\x01\xb9d\xe5\x8e&").into_buf();
        assert_eq!(b.get_leb128(), 2);
        assert_eq!(b.get_leb128(), 127);
        assert_eq!(b.get_leb128(), 128);
        assert_eq!(b.get_leb128(), 129);
        assert_eq!(b.get_leb128(), 130);
        assert_eq!(b.get_leb128(), 12857);
        assert_eq!(b.get_leb128(), 624485);
    }

    #[test]
    fn encode() {
        let mut b = BytesMut::with_capacity(128);
        b.put_leb128(2);
        b.put_leb128(127);
        b.put_leb128(128);
        b.put_leb128(129);
        b.put_leb128(130);
        b.put_leb128(12857);
        b.put_leb128(624485);
        assert_eq!(&*b, b"\x02\x7f\x80\x01\x81\x01\x82\x01\xb9d\xe5\x8e&");
    }
}
