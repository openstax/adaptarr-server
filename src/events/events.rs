use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Event {
    Assigned(Assigned),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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

/// A version of [`Event`] expanded to include additional information.
///
/// This enum is intended to be used where obtaining additional information
/// about an event would be difficult, for example inside an email template.
#[derive(Debug, Serialize)]
#[serde(tag = "kind")]
pub enum ExpandedEvent {
    #[serde(rename = "assigned")]
    Assigned {
        who: ExpandedUser,
        module: ExpandedModule,
    }
}

#[derive(Debug, Serialize)]
pub struct ExpandedUser {
    /// User's name.
    pub name: String,
    /// User's profile URL.
    pub url: String,
}

#[derive(Debug, Serialize)]
pub struct ExpandedModule {
    /// Module's title.
    pub title: String,
    /// Module's URL.
    pub url: String,
}
