use uuid::Uuid;

#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum Event {
    Assigned(Assigned),
}

#[derive(Clone, Debug, Serialize)]
pub struct Assigned {
    /// User who assigned.
    pub who: i32,
    /// Module to which the user was assigned.
    pub module: Uuid,
}

impl Event {
    pub fn kind(&self) -> &'static str {
        match *self {
            Event::Assigned(_) => "assigned",
        }
    }
}

impl_from! { for Event ;
    Assigned => |e| Event::Assigned(e),
}
