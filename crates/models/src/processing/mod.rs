//! Routines dedicated to processing uploaded data.

pub mod import;

mod xref_targets;

pub use self::{
    import::Importer,
    xref_targets::{ProcessDocument, TargetProcessor},
};
