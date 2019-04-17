use std::ops::Deref;

use crate::db::models as db;

/// Abstract representation of roles a user can take during an editing process.
#[derive(Debug)]
pub struct Slot {
    data: db::EditProcessSlot,
}

impl Slot {
    /// Construct `Slot` from its database counterpart.
    pub(super) fn from_db(data: db::EditProcessSlot) -> Slot {
        Slot { data }
    }
}

impl Deref for Slot {
    type Target = db::EditProcessSlot;

    fn deref(&self) -> &db::EditProcessSlot {
        &self.data
    }
}
