use crate::db::models as db;

/// A transition between two editing steps.
///
/// See [module description][super] for details.
#[derive(Debug)]
pub struct Link {
    data: db::EditProcessLink,
}

impl Link {
    /// Construct `Link` from its database counterpart.
    pub(super) fn from_db(data: db::EditProcessLink) -> Link {
        Link { data }
    }

    /// Unpack database data.
    pub fn into_db(self) -> db::EditProcessLink {
        self.data
    }
}

impl std::ops::Deref for Link {
    type Target = db::EditProcessLink;

    fn deref(&self) -> &db::EditProcessLink {
        &self.data
    }
}
