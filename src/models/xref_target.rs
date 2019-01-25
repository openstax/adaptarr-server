use crate::db::{
    models as db,
};

/// A cross-reference target.
#[derive(Debug)]
pub struct XrefTarget {
    data: db::XrefTarget,
}

/// A subset of a cross-reference target's data that can safely be publicly
/// exposed.
#[derive(Debug, Serialize)]
pub struct PublicData {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub description: Option<String>,
    pub context: Option<String>,
    pub counter: i32,
}

impl XrefTarget {
    /// Construct `XrefTarget` from its database counterpart.
    pub(crate) fn from_db(data: db::XrefTarget) -> XrefTarget {
        XrefTarget { data }
    }

    /// Get the public portion of this reference target's data.
    pub fn get_public(&self) -> PublicData {
        PublicData {
            id: self.data.element.clone(),
            type_: self.data.type_.clone(),
            description: self.data.description.clone(),
            context: self.data.context.clone(),
            counter: self.data.counter,
        }
    }
}
