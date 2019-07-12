use diesel_derive_enum::DbEnum;
use serde::{Deserialize, Serialize};

use std::fmt;

#[derive(Clone, Copy, DbEnum, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[DieselType = "Slot_permission"]
#[serde(rename_all = "kebab-case")]
pub enum SlotPermission {
    /// Permission holder can view the document.
    View,
    /// Permission holder can edit the document.
    ///
    /// This permission can only be granted to one slot. This permission cannot
    /// be granted if [`SlotPermission::ProposeChanges`] or
    /// [`SlotPermission::AcceptChanges`] is also granted.
    Edit,
    /// Permission holder can propose changes to the document.
    ///
    /// This permission can only be granted to one slot. This permission cannot
    /// be granted if [`SlotPermission::Edit`] or
    /// [`SlotPermission::AcceptChanges`] is also granted.
    ProposeChanges,
    /// Permission holder can accept changes proposed by a user with permission
    /// [`SlotPermission::ProposeChanges`].
    ///
    /// This permission can only be granted to one slot. This permission cannot
    /// be granted if [`SlotPermission::Edit`] or
    /// [`SlotPermission::ProposeChanges`] is also granted.
    AcceptChanges
}

impl SlotPermission {
    /// Does this permission allow editing documents?
    pub fn allows_editing(self) -> bool {
        match self {
            SlotPermission::Edit | SlotPermission::ProposeChanges |
            SlotPermission::AcceptChanges => true,
            _ => false,
        }
    }
}

impl fmt::Display for SlotPermission {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.write_str(match *self {
            SlotPermission::View => "view",
            SlotPermission::Edit => "edit",
            SlotPermission::ProposeChanges => "propose-changes",
            SlotPermission::AcceptChanges => "accept-changes",
        })
    }
}
