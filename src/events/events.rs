use uuid::Uuid;

use crate::{
    config::Config,
    db::{
        Connection,
        models as db,
    },
    models::{
        user::{User, FindUserError},
        module::{Module, FindModuleError},
    },
};
use super::Error;

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

pub fn expand_event(config: &Config, dbcon: &Connection, event: &db::Event)
-> Result<ExpandedEvent, Error> {
    let data = rmps::from_slice(&event.data)?;

    Ok(match data {
        Event::Assigned(ev) => {
            let who = match User::by_id(dbcon, ev.who) {
                Ok(user) => user,
                Err(FindUserError::Internal(err)) =>
                    return Err(err.into()),
                Err(FindUserError::NotFound) => panic!(
                    "Inconsistent database: user doesn't exist \
                    but is referenced by an event",
                ),
            }.into_db();
            let module = match Module::by_id(dbcon, ev.module) {
                Ok(module) => module,
                Err(FindModuleError::Database(err)) =>
                    return Err(err.into()),
                Err(FindModuleError::NotFound) => panic!(
                    "Inconsistent database: module doesn't exist \
                    but is referenced by an event",
                ),
            }.into_db();

            ExpandedEvent::Assigned {
                who: ExpandedUser {
                    name: who.0.name,
                    url: format!("https://{}/users/{}",
                        config.server.domain, who.0.id),
                },
                module: ExpandedModule {
                    title: module.1.title,
                    url: format!("https://{}/modules/{}",
                        config.server.domain, module.0.id),
                },
            }
        }
    })
}
