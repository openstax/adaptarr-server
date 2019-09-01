use serde::Serialize;
use std::convert::Infallible;

use crate::db::{
    Connection,
    models as db,
};
use super::{FindModelResult, Model};

/// A cross-reference target.
#[derive(Debug)]
pub struct XrefTarget {
    data: db::XrefTarget,
}

/// A subset of a cross-reference target's data that can safely be publicly
/// exposed.
#[derive(Debug, Serialize)]
pub struct Public {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub description: Option<String>,
    pub context: Option<String>,
    pub counter: i32,
}

impl Model for XrefTarget {
    const ERROR_CATEGORY: &'static str = "xref-target";

    type Id = Infallible;
    type Database = db::XrefTarget;
    type Public = Public;
    type PublicParams = ();

    fn by_id(_: &Connection, _: Infallible) -> FindModelResult<Self> {
        unreachable!()
    }

    fn from_db(data: Self::Database) -> Self {
        XrefTarget { data }
    }

    fn into_db(self) -> Self::Database {
        self.data
    }

    fn id(&self) -> Self::Id {
        unreachable!()
    }

    fn get_public(&self) -> Self::Public {
        Public {
            id: self.data.element.clone(),
            type_: self.data.type_.clone(),
            description: self.data.description.clone(),
            context: self.data.context.clone(),
            counter: self.data.counter,
        }
    }
}
