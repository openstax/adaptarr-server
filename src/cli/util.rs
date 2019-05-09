use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use termion::style::{Underline, Reset};
use std::fmt;

use crate::{
    Result,
    permissions::PermissionBits,
};

pub fn parse_permissions(v: &str) -> Result<PermissionBits> {
    use serde::de::{Deserialize, IntoDeserializer, value::Error};

    v.split(',')
        .map(str::trim)
        .map(IntoDeserializer::into_deserializer)
        .map(PermissionBits::deserialize)
        .collect::<Result<PermissionBits, Error>>()
        .map_err(Into::into)
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
