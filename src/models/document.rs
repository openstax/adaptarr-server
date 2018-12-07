use crate::db::models as db;

/// Document model serves as a shared foundation for modules and drafts. You
/// don't construct a `Document`, but obtain it via [`std::ops::Deref`] from
/// modules or drafts.
#[derive(Debug)]
pub struct Document {
    data: db::Document,
}

impl Document {
    /// Construct `Document` from its database counterpart.
    pub(super) fn from_db(data: db::Document) -> Document {
        Document { data }
    }
}

impl std::ops::Deref for Document {
    type Target = db::Document;

    fn deref(&self) -> &db::Document {
        &self.data
    }
}
