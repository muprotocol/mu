use std::{fmt::Display, ops::Deref, str::FromStr};

use crate::mu_stack::StackID;
use serde::{Deserialize, Serialize};
use sled::IVec;

use super::{
    doc_filter::DocFilter,
    update::{Changes, Update},
};

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
            _ => Err(super::Error::IndexAttrShouldBeString(v.to_string())),
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Doc {
    raw: String,
    json: serde_json::Value,
}

impl Doc {
    pub fn filter(self, filter: &DocFilter) -> Option<Self> {
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
}

impl Deref for Doc {
    type Target = serde_json::Value;

    fn deref(&self) -> &Self::Target {
        &self.json
    }
}

impl Update for Doc {
    fn doc(&mut self) -> &mut serde_json::Value {
        &mut self.json
    }

    fn finalize(self, changes: &Changes) -> Self {
        let mut value = self;
        if !changes.is_empty() {
            value.raw = value.json.to_string();
        }
        value
    }
}

impl TryFrom<serde_json::Value> for Doc {
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

impl From<Doc> for serde_json::Value {
    fn from(v: Doc) -> Self {
        v.json
    }
}

impl From<Doc> for String {
    fn from(value: Doc) -> Self {
        value.raw
    }
}

impl TryFrom<String> for Doc {
    type Error = serde_json::Error;
    fn try_from(raw: String) -> Result<Self, Self::Error> {
        let json = serde_json::from_str(&raw)?;
        Ok(Self { raw, json })
    }
}

impl TryFrom<&str> for Doc {
    type Error = serde_json::Error;
    fn try_from(raw: &str) -> Result<Self, Self::Error> {
        Ok(Self {
            raw: raw.to_string(),
            json: serde_json::from_str(raw)?,
        })
    }
}

impl TryFrom<IVec> for Doc {
    type Error = String;
    fn try_from(ivec: IVec) -> Result<Self, Self::Error> {
        String::from_utf8(ivec.to_vec())
            .map_err(|e| e.to_string())?
            .try_into()
            .map_err(|e: serde_json::Error| e.to_string())
    }
}

impl From<Doc> for IVec {
    fn from(value: Doc) -> Self {
        value.raw.as_str().into()
    }
}

// Item

pub type Item = (Key, Doc);

// KeyFilter

pub type KeyFilter = GenericKeyFilter<String>;
pub type KfBy = GenericKfBy<String>;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub enum GenericKeyFilter<T: Into<Key>> {
    /// primary key
    PK(GenericKfBy<T>),
    /// secondary key
    SK(String, GenericKfBy<T>),
}

/// key filter by exact or prefix
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub enum GenericKfBy<T: Into<Key>> {
    Exact(T),
    Prefix(String),
}

// Indexes

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct Indexes {
    /// primary key
    pub pk_attr: String,
    /// secondary keys
    pub sk_attr_list: Vec<String>,
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

// TableDescription

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct TableDescription {
    pub table_name: String,
    pub indexes: Indexes,
    // TODO
    // pub creation_date_time: DateTime,
}

impl From<TableDescription> for Doc {
    fn from(td: TableDescription) -> Self {
        let json = serde_json::to_value(td).unwrap();
        Self::try_from(json).unwrap()
    }
}

impl TryFrom<Doc> for TableDescription {
    type Error = serde_json::Error;
    fn try_from(value: Doc) -> Result<Self, Self::Error> {
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
