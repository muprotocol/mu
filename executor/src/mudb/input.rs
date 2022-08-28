/* Data structures about inputs
 * TODO: refactor validation
 */

//! User API

use super::{
    error::ValidationResult,
    query::{Filter, Update},
};

use serde::Deserialize;
use validator::{Validate, ValidationError};

pub(crate) const TABLES_DESCRIPTIONS_TABLE: &str = "tables_descriptions";
pub(crate) const RESERVED_TABLES: [&str; 1] = [TABLES_DESCRIPTIONS_TABLE];

pub type Key = String;
pub type Value = String;
pub type Item = (Key, Value);

#[derive(Clone, Debug, Deserialize)]
pub enum KeyFilter {
    Exact(Key),
    Prefix(String),
}

#[derive(Clone, Debug, Validate, Deserialize)]
pub struct CreateTableInput {
    #[validate(length(min = 1), custom = "validate_no_reserved_table")]
    pub table_name: String,
}

#[derive(Clone, Debug, Validate, Deserialize)]
pub struct DeleteTableInput {
    #[validate(length(min = 1), custom = "validate_no_reserved_table")]
    pub table_name: String,
}

#[derive(Clone, Debug, Validate, Deserialize)]
pub struct InsertOneItemInput {
    #[validate(length(min = 1), custom = "validate_no_reserved_table")]
    pub table_name: String,
    pub key: Key,
    pub value: Value,
}

// TODO
// #[derive(Debug, Validate, Deserialize)]
// pub struct InsertManyInput {
//     #[validate(length(min = 1), custom = "validate_no_reserved_table")]
//     table_name: String,
//     items: Vec<Item>,
// }

#[derive(Debug, Validate, Deserialize)]
pub struct FindItemInput {
    #[validate(length(min = 1), custom = "validate_no_reserved_table")]
    pub table_name: String,
    pub key_filter: KeyFilter,
    #[validate(custom = "validate_filter")]
    pub filter: Option<Filter>,
}

#[derive(Debug, Validate, Deserialize)]
pub struct UpdateItemInput {
    #[validate(length(min = 1), custom = "validate_no_reserved_table")]
    pub table_name: String,
    pub key_filter: KeyFilter,
    #[validate(custom = "validate_filter")]
    pub filter: Option<Filter>,
    #[validate(custom = "validate_update")]
    pub update: Update,
}

#[derive(Debug, Validate, Deserialize)]
pub struct DeleteItemInput {
    #[validate(length(min = 1), custom = "validate_no_reserved_table")]
    pub table_name: String,
    pub key_filter: KeyFilter,
    #[validate(custom = "validate_filter")]
    pub filter: Option<Filter>,
}

#[derive(Debug, Validate, Deserialize)]
pub struct DeleteAllItemsInput {
    #[validate(length(min = 1), custom = "validate_no_reserved_table")]
    pub table_name: String,
}

#[derive(Debug, Validate, Deserialize)]
pub struct TableLenInput {
    #[validate(length(min = 1), custom = "validate_no_reserved_table")]
    pub table_name: String,
}

#[derive(Debug, Validate, Deserialize)]
pub struct TableIsEmptyInput {
    #[validate(length(min = 1), custom = "validate_no_reserved_table")]
    pub table_name: String,
}

fn validate_no_reserved_table(table_name: &str) -> ValidationResult<()> {
    match RESERVED_TABLES.contains(&table_name) {
        true => Err(ValidationError::new("table_is_reserved")),
        false => Ok(()),
    }
}

fn validate_update(update: &Update) -> ValidationResult<()> {
    update.validate()
}

fn validate_filter(filter: &Filter) -> ValidationResult<()> {
    filter.validate()
}
