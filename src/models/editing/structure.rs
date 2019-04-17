pub use crate::db::types::SlotPermission;

#[derive(Debug, Serialize)]
pub struct Process {
    /// Process's name.
    pub name: String,
    /// ID of the initial step.
    pub start: usize,
    /// Slots defined for this process.
    pub slots: Vec<Slot>,
    /// Steps in this process.
    pub steps: Vec<Step>,
}

#[derive(Debug, Serialize)]
pub struct Slot {
    /// Database ID of this slot.
    #[serde(skip_deserializing)]
    pub id: i32,
    pub name: String,
    #[serde(default)]
    pub role: Option<i32>,
    #[serde(default)]
    pub autofill: bool,
}

#[derive(Debug, Serialize)]
pub struct Step {
    /// Database ID of this step.
    #[serde(skip_deserializing)]
    pub id: i32,
    pub name: String,
    #[serde(default)]
    pub slots: Vec<StepSlot>,
    #[serde(default)]
    pub links: Vec<Link>,
}

#[derive(Debug, Serialize)]
pub struct StepSlot {
    pub slot: usize,
    pub permission: SlotPermission,
}

#[derive(Debug, Serialize)]
pub struct Link {
    pub name: String,
    pub to: usize,
    pub slot: usize,
}
