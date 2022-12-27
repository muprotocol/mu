use std::borrow::Cow;

use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Debug, BorshDeserialize, BorshSerialize)]
pub enum DatabaseRequest<'a> {
    CreateTable(CreateTableRequest<'a>),
    DropTable(DropTableRequest<'a>),
    Find(FindRequest<'a>),
    Insert(InsertRequest<'a>),
    Update(UpdateRequest<'a>),
}

#[derive(Debug, BorshDeserialize, BorshSerialize)]
pub enum KeyFilter<'a> {
    Exact(Cow<'a, str>),
    Prefix(Cow<'a, str>),
}

macro_rules! make_request {
    ($name:ident$(, $field:ident : $type:ty)*) => {
        #[derive(Debug, BorshDeserialize, BorshSerialize)]
        pub struct $name<'a> {
            pub db_name: Cow<'a, str>,
            pub table_name: Cow<'a, str>,
            $(
            pub $field: $type,
            )*
        }
    };
}

make_request!(CreateTableRequest);
make_request!(DropTableRequest);
make_request!(
    FindRequest,
    key_filter: KeyFilter<'a>,
    value_filter: Cow<'a, str>
);
make_request!(InsertRequest, key: Cow<'a, str>, value: Cow<'a, str>);
make_request!(
    UpdateRequest,
    key_filter: KeyFilter<'a>,
    value_filter: Cow<'a, str>,
    update: Cow<'a, str>
);

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub enum DatabaseResponse<'a> {
    CreateTable(Result<TableDescription<'a>, String>),
    DropTable(Result<Option<TableDescription<'a>>, String>),
    Find(Result<Vec<Item<'a>>, String>),
    Insert(Result<Cow<'a, str>, String>),
    Update(Result<Vec<Item<'a>>, String>),
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct TableDescription<'a> {
    pub table_name: Cow<'a, str>,
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub struct Item<'a> {
    pub key: Cow<'a, str>,
    pub value: Cow<'a, str>,
}
