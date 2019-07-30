use bytes::{Buf, BufMut};

pub trait BufExt: Buf {
    fn get_leb128(&mut self) -> u64;
}

impl<T: Buf> BufExt for T {
    fn get_leb128(&mut self) -> u64 {
        let mut buf = [0; 1];
        let mut v = 0u64;

        loop {
            self.copy_to_slice(&mut buf);
            v = v.checked_shl(7)
                .expect("overflow")
                .checked_add(u64::from(buf[0] & 0xff))
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
