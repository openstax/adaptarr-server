#[derive(Clone, Debug, Serialize)]
#[serde(untagged)]
pub enum Event {
}

impl Event {
    pub fn kind(&self) -> &'static str {
        match *self {
        }
    }
}
