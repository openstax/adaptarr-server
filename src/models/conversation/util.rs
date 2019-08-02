use bytes::{Buf, BufMut};

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
