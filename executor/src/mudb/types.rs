use std::{fmt::Display, ops::Deref, str::FromStr};

use crate::mu_stack::StackID;
use serde::{Deserialize, Serialize};
use sled::IVec;

use super::{update::ChangedSections, Updater, ValueFilter};

pub(crate) const MANAGER_DB: &str = "mudb_manager";
pub(crate) const DB_DESCRIPTION_TABLE: &str = "db_list";
pub(crate) const TABLE_DESCRIPTIONS_TABLE: &str = "table_descriptions";
pub(crate) const RESERVED_TABLES: [&str; 1] = [TABLE_DESCRIPTIONS_TABLE];

// Key

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key(String);

impl Deref for Key {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_str()
    }
}

impl From<&str> for Key {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}

impl From<String> for Key {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl TryFrom<&serde_json::Value> for Key {
    type Error = super::Error;
    fn try_from(v: &serde_json::Value) -> Result<Self, Self::Error> {
        match v {
            serde_json::Value::String(s) => Ok(s.as_str().into()),
            _ => Err(super::Error::IndexAttributeShouldBeString(v.to_string())),
        }
    }
}

impl From<Key> for String {
    fn from(value: Key) -> Self {
        value.0
    }
}

impl From<Key> for IVec {
    fn from(value: Key) -> Self {
        value.deref().into()
    }
}

impl TryFrom<IVec> for Key {
    type Error = std::string::FromUtf8Error;
    fn try_from(value: IVec) -> Result<Self, Self::Error> {
        Ok(Self(String::from_utf8(value.to_vec())?))
    }
}

impl AsRef<[u8]> for Key {
    fn as_ref(&self) -> &[u8] {
        self.deref().as_ref()
    }
}

impl Display for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.deref().fmt(f)
    }
}

// Value

// TODO: rename to Item after remove Key
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Value {
    // TODO: rename to string
    raw: String,
    json: serde_json::Value,
}

impl Value {
    pub fn raw(&self) -> &str {
        self.raw.as_str()
    }

    pub fn filter(self, filter: &ValueFilter) -> Option<Self> {
        if filter.eval(&self.json) {
            Some(self)
        } else {
            None
        }
        // match filter {
        //     Some(filter) if !filter.eval(&self.json) => None,
        //     _ => Some(self),
        // }
    }

    pub fn update(self, updater: &Updater) -> (Self, ChangedSections) {
        let mut value = self;
        let u_res = updater.update(&mut value.json);
        if !u_res.is_empty() {
            value.raw = value.json.to_string();
        }
        (value, u_res)
    }
}

impl Deref for Value {
    type Target = serde_json::Value;

    fn deref(&self) -> &Self::Target {
        &self.json
    }
}

// impl From<serde_json::Value> for Value {
//     fn from(json: serde_json::Value) -> Self {
//         Self {
//             raw: json.to_string(),
//             json,
//         }
//     }
// }

impl TryFrom<serde_json::Value> for Value {
    type Error = super::Error;
    fn try_from(json: serde_json::Value) -> Result<Self, Self::Error> {
        if json.is_object() {
            let raw = json.to_string();
            Ok(Self { raw, json })
        } else {
            Err(super::Error::ExpectedObjectValue(json.to_string()))
        }
    }
}

impl From<Value> for serde_json::Value {
    fn from(v: Value) -> Self {
        v.json
    }
}

impl From<Value> for String {
    fn from(value: Value) -> Self {
        value.raw
    }
}

impl TryFrom<String> for Value {
    type Error = serde_json::Error;
    fn try_from(raw: String) -> Result<Self, Self::Error> {
        let json = serde_json::from_str(&raw)?;
        Ok(Self { raw, json })
    }
}

impl TryFrom<&str> for Value {
    type Error = serde_json::Error;
    fn try_from(raw: &str) -> Result<Self, Self::Error> {
        Ok(Self {
            raw: raw.to_string(),
            json: serde_json::from_str(raw)?,
        })
    }
}

impl TryFrom<IVec> for Value {
    type Error = String;
    fn try_from(ivec: IVec) -> Result<Self, Self::Error> {
        String::from_utf8(ivec.to_vec())
            .map_err(|e| e.to_string())?
            .try_into()
            .map_err(|e: serde_json::Error| e.to_string())
    }
}

impl From<Value> for IVec {
    fn from(value: Value) -> Self {
        value.raw.as_str().into()
    }
}

// Item

pub type Item = (Key, Value);

// KeyFilter

pub type KeyFilter = KeyFilterFrom<String>;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub enum KeyFilterFrom<T: Into<Key>> {
    Exact(T),
    Prefix(String),
}

// TableName

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableNameInput(String);

impl Deref for TableNameInput {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.0.as_str()
    }
}

impl TryFrom<String> for TableNameInput {
    type Error = super::Error;
    fn try_from(s: String) -> Result<Self, Self::Error> {
        if RESERVED_TABLES.contains(&s.as_str()) {
            Err(Self::Error::InvalidTableName(s, "is reserved".into()))
        } else if s.is_empty() {
            Err(Self::Error::InvalidTableName(s, "can't be empty".into()))
        } else {
            Ok(Self(s))
        }
    }
}

impl TryFrom<&str> for TableNameInput {
    type Error = super::Error;
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::try_from(String::from(s))
    }
}

impl From<TableNameInput> for Key {
    fn from(tn: TableNameInput) -> Self {
        Self::from(tn.deref())
    }
}

impl From<TableNameInput> for String {
    fn from(tb: TableNameInput) -> Self {
        tb.0
    }
}

impl AsRef<[u8]> for TableNameInput {
    fn as_ref(&self) -> &[u8] {
        self.deref().as_ref()
    }
}

// Schema

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct Indexes {
    // TODO: rename to pk_attr
    /// primary key
    pub pk: String,
}

// TODO
// pub trait Schema {
//     fn primary_key() ->  {
//
//     }
// }

// TableDescription

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct TableDescription {
    pub table_name: String,
    pub indexes: Indexes,
    // TODO
    // pub creation_date_time: DateTime,
}

impl From<TableDescription> for Value {
    fn from(td: TableDescription) -> Self {
        let json = serde_json::to_value(td).unwrap();
        Self::try_from(json).unwrap()
    }
}

impl TryFrom<Value> for TableDescription {
    type Error = serde_json::Error;
    fn try_from(value: Value) -> Result<Self, Self::Error> {
        serde_json::from_value(value.json)
    }
}

// DatabaseID

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseID {
    pub stack_id: StackID,
    pub db_name: String,
}

impl Default for DatabaseID {
    fn default() -> Self {
        Self {
            stack_id: StackID(uuid::Uuid::nil()),
            db_name: "default.mudb".into(),
        }
    }
}

impl ToString for DatabaseID {
    fn to_string(&self) -> String {
        format!("{}_{}", self.stack_id, self.db_name.replace(' ', "-"))
    }
}

impl FromStr for DatabaseID {
    type Err = super::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use super::Error::InvalidDbId;
        match s.split_once('_') {
            Some((stack_id, db_name)) => Ok(Self {
                stack_id: StackID(
                    uuid::Uuid::try_parse(stack_id).map_err(|e| InvalidDbId(e.to_string()))?,
                ),
                db_name: db_name.to_string(),
            }),
            None => Err(InvalidDbId(format!("not found '_' in {s}"))),
        }
    }
}
