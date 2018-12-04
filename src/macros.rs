/// Auto-implement [`From`] for a type.
#[macro_export]
macro_rules! impl_from {
    { for $type:ty ;
        $(
            $from:ty => | $pat:pat | $value:expr
        ),+
        $(,)*
    } => {
        $(
            impl From<$from> for $type {
                fn from(f: $from) -> $type {
                    let $pat = f;
                    $value
                }
            }
        )+
    };
}
