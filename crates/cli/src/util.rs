use adaptarr_models::PermissionBits;
use serde::{de::{DeserializeOwned, IntoDeserializer}, ser};
use std::fmt::{self, Write as _};
use termion::style::{Underline, Reset};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::Result;

pub fn parse_permissions<P>(v: &str) -> Result<P>
where
    P: DeserializeOwned + PermissionBits,
{
    let mut permissions = P::empty();

    let iter = v.split(',')
        .map(str::trim)
        .map(<&str as IntoDeserializer<serde::de::value::Error>>::into_deserializer)
        .map(P::deserialize);

    for permission in iter {
        permissions.insert(permission?);
    }

    Ok(permissions)
}

pub fn format<T: ser::Serialize>(t: T) -> String {
    let mut ser = StringSerializer::default();
    t.serialize(&mut ser).unwrap();
    ser.inner
}

pub fn print_table<H, T, R>(header: H, rows: T)
where
    H: TableRow,
    T: AsRef<[R]>,
    R: TableRow<Size = H::Size>,
{
    let mut widths = vec![0; H::size()];

    for (inx, width) in widths.iter_mut().enumerate().take(H::size()) {
        *width = UnicodeWidthStr::width(header.column(inx));
    }

    for row in rows.as_ref() {
        for (inx, width) in widths.iter_mut().enumerate().take(H::size()) {
            *width = (*width).max(UnicodeWidthStr::width(row.column(inx)));
        }
    }

    // Sum of all longest widths and spaces separating them.
    let total_width = widths.iter().cloned().sum::<usize>() + widths.len() - 1;

    let (terminal_width, _) = termion::terminal_size().unwrap_or((80, 20));
    let terminal_width = usize::from(terminal_width);

    if total_width >= terminal_width {
        let overflow = total_width - terminal_width;
        let last = widths.last_mut().unwrap();

        if overflow < *last {
            *last -= overflow;
        } else {
            panic!("Can't render table: terminal is too small ({} < {})",
                terminal_width, total_width);
        }
    }

    for (inx, width) in widths.iter().enumerate().take(H::size()) {
        if inx > 0 {
            print!(" ");
        }
        print!("{}{}{}",
            Underline, Column(header.column(inx), *width), Reset);
    }
    println!();

    for row in rows.as_ref() {
        for (inx, width) in widths.iter().enumerate().take(H::size()) {
            if inx > 0 {
                print!(" ");
            }
            print!("{}", Column(row.column(inx), *width));
        }
        println!();
    }
}

pub trait TableRow {
    type Size;

    fn size() -> usize;

    fn column(&self, index: usize) -> &str;
}

macro_rules! impl_table_row {
    {
        $(
            $sizeconst:literal $size:ident => $($inx:tt : $ty:ident),+
        );+
        $(;)*
    } => {
        $(
            pub struct $size;

            impl<$($ty),+> TableRow for ($($ty,)+)
            where
                $($ty: AsRef<str>),+
            {
                type Size = $size;

                fn size() -> usize { $sizeconst }

                fn column(&self, index: usize) -> &str {
                    match index {
                        $($inx => self.$inx.as_ref(),)+
                        _ => panic!("Index {} out of range", index),
                    }
                }
            }
        )+
    };
}

impl_table_row! {
    1 One   => 0: A;
    2 Two   => 0: A, 1: B;
    3 Three => 0: A, 1: B, 2: C;
    4 Four  => 0: A, 1: B, 2: C, 3: D;
    5 Five  => 0: A, 1: B, 2: C, 3: D, 4: E;
    6 Six   => 0: A, 1: B, 2: C, 3: D, 4: E, 5: F;
}

struct Column<'a>(&'a str, usize);

impl<'a> fmt::Display for Column<'a> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        let (len, end) = self.0.char_indices()
            .scan(0, |total_len, (inx, chr)| {
                *total_len += UnicodeWidthChar::width(chr).unwrap_or(0);
                if *total_len > self.1 {
                    None
                } else {
                    Some((*total_len, inx + chr.len_utf8()))
                }
            })
            .last()
            .unwrap_or((0, 0));

        let pad = if len >= self.1 {
            0
        } else {
            self.1 - len
        };

        write!(fmt, "{0}{1:2$}", &self.0[..end], "", pad)
    }
}

#[derive(Default)]
struct StringSerializer {
    inner: String,
    add_comma: bool,
}

impl<'a> ser::Serializer for &'a mut StringSerializer {
    type Ok = ();
    type Error = serde::de::value::Error;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        self.inner.push_str(if v { "true" } else { "false" });
        Ok(())
    }

    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        self.serialize_i64(i64::from(v))
    }

    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        self.serialize_i64(i64::from(v))
    }

    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        self.serialize_i64(i64::from(v))
    }

    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        let _ = write!(self.inner, "{}", v);
        Ok(())
    }

    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        self.serialize_u64(u64::from(v))
    }

    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        self.serialize_u64(u64::from(v))
    }

    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        self.serialize_u64(u64::from(v))
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        let _ = write!(self.inner, "{}", v);
        Ok(())
    }

    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> {
        self.serialize_f64(f64::from(v))
    }

    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
        let _ = write!(self.inner, "{}", v);
        Ok(())
    }

    fn serialize_char(self, v: char) -> Result<Self::Ok, Self::Error> {
        self.inner.push(v);
        Ok(())
    }

    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        self.inner.push_str(v);
        Ok(())
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        let _ = write!(self.inner, "{:?}", v);
        Ok(())
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.inner.push_str("none");
        Ok(())
    }

    fn serialize_some<T>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_unit_struct(self, _: &'static str)
    -> Result<Self::Ok, Self::Error> {
        Ok(())
    }

    fn serialize_unit_variant(self, _: &'static str, _: u32, variant: &'static str)
    -> Result<Self::Ok, Self::Error> {
        self.inner.push_str(variant);
        Ok(())
    }

    fn serialize_newtype_struct<T>(self, _: &'static str, value: &T)
    -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T>(
        self,
        _: &'static str,
        _: u32,
        _: &'static str,
        value: &T
    ) -> Result<Self::Ok, Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        value.serialize(self)
    }

    fn serialize_seq(self, _: Option<usize>)
    -> Result<Self::SerializeSeq, Self::Error> {
        Ok(self)
    }

    fn serialize_tuple(self, _: usize)
    -> Result<Self::SerializeTuple, Self::Error> {
        Ok(self)
    }

    fn serialize_tuple_struct(self, _: &'static str, _: usize)
    -> Result<Self::SerializeTupleStruct, Self::Error> {
        Ok(self)
    }

    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        self.inner.push_str(variant);
        Ok(self)
    }

    fn serialize_map(self, _: Option<usize>)
    -> Result<Self::SerializeMap, Self::Error> {
        Ok(self)
    }

    fn serialize_struct(self, _: &'static str, _: usize)
    -> Result<Self::SerializeStruct, Self::Error> {
        Ok(self)
    }

    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        self.inner.push_str(variant);
        Ok(self)
    }
}

impl<'a> ser::SerializeSeq for &'a mut StringSerializer {
    type Ok = ();
    type Error = serde::de::value::Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        if self.add_comma {
            self.inner.push_str(", ");
        }
        self.add_comma = true;
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'a> ser::SerializeTuple for &'a mut StringSerializer {
    type Ok = ();
    type Error = serde::de::value::Error;

    fn serialize_element<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        if self.add_comma {
            self.inner.push_str(", ");
        }
        self.add_comma = true;
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'a> ser::SerializeTupleStruct for &'a mut StringSerializer {
    type Ok = ();
    type Error = serde::de::value::Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        if self.add_comma {
            self.inner.push_str(", ");
        }
        self.add_comma = true;
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'a> ser::SerializeTupleVariant for &'a mut StringSerializer {
    type Ok = ();
    type Error = serde::de::value::Error;

    fn serialize_field<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        if self.add_comma {
            self.inner.push_str(", ");
        }
        self.add_comma = true;
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'a> ser::SerializeMap for &'a mut StringSerializer {
    type Ok = ();
    type Error = serde::de::value::Error;

    fn serialize_key<T>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        if self.add_comma {
            self.inner.push_str(", ");
        }
        self.add_comma = true;
        key.serialize(&mut **self)?;
        self.inner.push_str(": ");
        Ok(())
    }

    fn serialize_value<T>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'a> ser::SerializeStruct for &'a mut StringSerializer {
    type Ok = ();
    type Error = serde::de::value::Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T)
        -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        if self.add_comma {
            self.inner.push_str(", ");
        }
        self.add_comma = true;
        self.inner.push_str(key);
        self.inner.push_str(": ");
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<'a> ser::SerializeStructVariant for &'a mut StringSerializer {
    type Ok = ();
    type Error = serde::de::value::Error;

    fn serialize_field<T>(&mut self, key: &'static str, value: &T)
        -> Result<(), Self::Error>
    where
        T: ?Sized + ser::Serialize,
    {
        if self.add_comma {
            self.inner.push_str(", ");
        }
        self.add_comma = true;
        self.inner.push_str(key);
        self.inner.push_str(": ");
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(())
    }
}
