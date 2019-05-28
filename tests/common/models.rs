//! Models for JSON objects used in API.

#![allow(dead_code)]

use adaptarr::{
    db::types::SlotPermission,
    models::bookpart::NewTree,
    permissions::PermissionBits,
};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use uuid::Uuid;

#[derive(Serialize)]
pub struct AdvanceDraft {
    pub target: i32,
    pub slot: i32,
}

#[derive(Serialize)]
pub struct AssignToSlot {
    pub draft: Uuid,
    pub slot: i32,
}

#[derive(Serialize)]
pub struct BeginProcess {
    pub process: i32,
    pub slots: Vec<(i32, i32)>,
}

#[derive(Debug, Serialize)]
pub struct BookChange<'a> {
    pub title: &'a str,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct BookData<'a> {
    pub id: Uuid,
    pub title: Cow<'a, str>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct DocumentData<'a> {
    pub title: Cow<'a, str>,
    pub language: Cow<'a, str>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct DraftData<'a> {
    pub module: Uuid,
    #[serde(flatten)]
    pub document: DocumentData<'a>,
    #[serde(default)]
    pub permissions: Vec<SlotPermission>,
    #[serde(default)]
    pub step: Option<StepData<'a>>,
}

#[derive(Debug, Serialize)]
pub struct DraftUpdate<'a> {
    pub title: &'a str,
}

#[derive(Serialize)]
pub struct ElevateCredentials<'a> {
    pub password: &'a str,
    pub next: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<adaptarr::api::pages::LoginAction>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct FileInfo<'a> {
    pub name: Cow<'a, str>,
    pub mime: Cow<'a, str>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct FreeSlot<'a> {
    #[serde(flatten)]
    pub slot: SlotData<'a>,
    pub draft: DraftData<'a>,
}

#[derive(Serialize)]
pub struct InviteParams<'a> {
    pub email: &'a str,
    pub language: &'a str,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct LinkData<'a> {
    pub name: Cow<'a, str>,
    pub to: i32,
    pub slot: i32,
}

#[derive(Serialize)]
pub struct LoginCredentials<'a> {
    pub email: &'a str,
    pub password: &'a str,
    pub next: Option<&'a str>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct ModuleData<'a> {
    pub id: Uuid,
    #[serde(flatten)]
    pub document: DocumentData<'a>,
}

#[derive(Debug, Serialize)]
pub struct ModuleUpdate {
}

#[derive(Debug, Serialize)]
pub struct NewBook<'a> {
    pub title: &'a str,
}

#[derive(Debug, Serialize)]
pub struct NewModule<'a> {
    pub title: &'a str,
    pub language: &'a str,
}

#[derive(Serialize)]
pub struct NewRole<'a> {
    pub name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionBits>
}

#[derive(Debug, Serialize)]
pub struct NewTreeRoot {
    #[serde(flatten)]
    pub tree: NewTree,
    pub parent: i32,
    pub index: i32,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct PartData<'a> {
    pub number: i32,
    pub title: Cow<'a, str>,
    #[serde(flatten)]
    pub part: Variant<i32>,
}

#[derive(Debug, Serialize)]
pub struct PartLocation {
    pub parent: i32,
    pub index: i32,
}

#[derive(Debug, Serialize)]
pub struct PartUpdate<'a> {
    pub title: &'a str,
    #[serde(flatten)]
    pub location: PartLocation,
}

#[derive(Serialize)]
pub struct PasswordChangeRequest<'a> {
    pub current: &'a str,
    pub new: &'a str,
    pub new2: &'a str,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct ProcessData<'a> {
    pub id: i32,
    pub name: Cow<'a, str>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct ProcessDetails<'a> {
    #[serde(flatten)]
    pub process: VersionData<'a>,
    pub slots: Vec<SlotSeating<'a>>,
}

#[derive(Serialize)]
pub struct ProcessUpdate<'a> {
    pub name: &'a str,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct RoleData<'a> {
    pub id: i32,
    pub name: Cow<'a, str>,
    pub permissions: Option<PermissionBits>,
}

#[derive(Serialize)]
pub struct RoleUpdate<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionBits>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct SessionData {
    pub expires: NaiveDateTime,
    pub is_elevated: bool,
    pub permissions: PermissionBits,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct SlotData<'a> {
    pub id: i32,
    pub name: Cow<'a, str>,
    pub role: Option<i32>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct SlotSeating<'a> {
    #[serde(flatten)]
    pub slot: SlotData<'a>,
    pub user: Option<UserData<'a>>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct StepData<'a> {
    pub id: i32,
    pub process: [i32; 2],
    pub name: Cow<'a, str>,
    pub links: Vec<LinkData<'a>>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct Tree<'a> {
    pub number: i32,
    pub title: Cow<'a, str>,
    #[serde(flatten)]
    pub part: Variant<Tree<'a>>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct UserData<'a> {
    pub id: i32,
    pub name: Cow<'a, str>,
    pub is_super: bool,
    pub language: Cow<'a, str>,
    pub permissions: Option<PermissionBits>,
    pub role: Option<RoleData<'a>>,
}

#[derive(Serialize)]
pub struct UserUpdate<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionBits>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Option<i32>>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Variant<Part> {
    Module {
        id: Uuid,
    },
    Group {
        parts: Vec<Part>,
    },
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct VersionData<'a> {
    pub id: i32,
    pub name: Cow<'a, str>,
    pub version: NaiveDateTime,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct XrefData<'a> {
    pub id: Cow<'a, str>,
    #[serde(rename = "type")]
    pub type_: Cow<'a, str>,
    pub description: Option<Cow<'a, str>>,
    pub context: Option<Cow<'a, str>>,
    pub counter: i32,
}
