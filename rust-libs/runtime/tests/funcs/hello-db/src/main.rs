use musdk::{
    db::{Key, TableName, Value},
    *,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
pub struct Create {
    pub table_name: String,
    pub key: String,
    pub value: String,
}

pub type Update = Create;

#[derive(Deserialize, Serialize, Debug)]
pub struct Read {
    pub table_name: String,
    pub key: String,
}

pub type Delete = Read;

fn blob_to_string(x: &[u8]) -> String {
    String::from_utf8_lossy(x).into_owned()
}

fn into_string_triple(x: (TableName, Key, Value)) -> (String, String, String) {
    (
        x.0.to_string(),
        blob_to_string(x.1.as_ref()),
        blob_to_string(x.2.as_ref()),
    )
}

#[mu_functions]
mod hello_db {
    use super::*;

    #[mu_function]
    fn table_list<'a>(ctx: &'a mut MuContext) -> Json<Vec<String>> {
        let x = ctx
            .db()
            .table_list("")
            .unwrap()
            .into_iter()
            .map(|x| x.to_string())
            .collect();

        Json(x)
    }

    #[mu_function]
    fn create<'a>(ctx: &'a mut MuContext, req: Json<Create>) {
        let req = req.into_inner();
        let table = &req.table_name;
        let key = req.key.as_bytes();
        let previous_value = Option::<&[u8]>::None;
        let new_value = req.value.as_bytes();
        // unique creation
        // create if previous value does not exist
        ctx.db()
            .compare_and_swap(table, key, previous_value, new_value)
            .unwrap();
    }

    #[mu_function]
    fn read<'a>(ctx: &'a mut MuContext, req: Json<Read>) -> String {
        let req = req.into_inner();
        ctx.db()
            .get(&req.table_name, req.key.as_bytes())
            .unwrap()
            .map(|x| blob_to_string(x.as_ref()))
            .unwrap_or("".into())
    }

    #[mu_function]
    fn update<'a>(ctx: &'a mut MuContext, req: Json<Update>) {
        let req = req.into_inner();
        let table = &req.table_name;
        let key = req.key.as_bytes();
        let value = req.value.as_bytes();
        let is_atomic = false;
        ctx.db().put(table, key, value, is_atomic).unwrap();
    }

    #[mu_function]
    fn delete<'a>(ctx: &'a mut MuContext, req: Json<Delete>) {
        let req = req.into_inner();
        let is_atomic = false;
        ctx.db()
            .delete(&req.table_name, req.key.as_bytes(), is_atomic)
            .unwrap()
    }

    #[mu_function]
    fn scan<'a>(
        ctx: &'a mut MuContext,
        req: Json<(String, String)>,
    ) -> Json<Vec<(String, String)>> {
        let req = req.into_inner();
        let limit = 15;
        let key_prefix = req.1.as_bytes();
        let table_name = &req.0;
        let res = ctx
            .db()
            .scan(table_name, key_prefix, limit)
            .unwrap()
            .into_iter()
            .map(|(k, v)| (blob_to_string(k.as_ref()), blob_to_string(v.as_ref())))
            .collect();

        Json(res)
    }

    #[mu_function]
    fn scan_keys<'a>(ctx: &'a mut MuContext, req: Json<(String, String)>) -> Json<Vec<String>> {
        let req = req.into_inner();
        let limit = 15;
        let key_prefix = req.1.as_bytes();
        let table_name = &req.0;
        let res = ctx
            .db()
            .scan_keys(table_name, key_prefix, limit)
            .unwrap()
            .into_iter()
            .map(|x| blob_to_string(x.as_ref()))
            .collect();

        Json(res)
    }

    #[mu_function]
    fn batch_put<'a>(ctx: &'a mut MuContext, req: Json<Vec<(String, String, String)>>) {
        let req = req.into_inner();
        let table_key_value_triples = req
            .iter()
            .map(|(x, y, z)| (x.as_str(), y.as_bytes(), z.as_bytes()))
            .collect::<Vec<_>>();
        let is_atomic = false;
        ctx.db()
            .batch_put(&table_key_value_triples, is_atomic)
            .unwrap()
    }

    #[mu_function]
    fn batch_get<'a>(
        ctx: &'a mut MuContext,
        req: Json<Vec<(String, String)>>,
    ) -> Json<Vec<(String, String, String)>> {
        let req = req.into_inner();
        let table_key_tuples = req
            .iter()
            .map(|(x, y)| (x.as_str(), y.as_bytes()))
            .collect::<Vec<_>>();
        let res = ctx
            .db()
            .batch_get(&table_key_tuples)
            .unwrap()
            .into_iter()
            .map(into_string_triple)
            .collect();
        Json(res)
    }

    #[mu_function]
    fn batch_scan<'a>(
        ctx: &'a mut MuContext,
        req: Json<Vec<(String, String)>>,
    ) -> Json<Vec<(String, String, String)>> {
        let req = req.into_inner();
        let table_key_prefix_tuples = req
            .iter()
            .map(|(x, y)| (x.as_str(), y.as_bytes()))
            .collect::<Vec<_>>();
        let each_limit = 32;
        let res = ctx
            .db()
            .batch_scan(&table_key_prefix_tuples, each_limit)
            .unwrap()
            .into_iter()
            .map(into_string_triple)
            .collect();
        Json(res)
    }

    #[mu_function]
    fn batch_scan_keys<'a>(
        ctx: &'a mut MuContext,
        req: Json<Vec<(String, String)>>,
    ) -> Json<Vec<(String, String)>> {
        let req = req.into_inner();
        let table_key_prefix_tuples = req
            .iter()
            .map(|(x, y)| (x.as_str(), y.as_bytes()))
            .collect::<Vec<_>>();
        let each_limit = 32;
        let res = ctx
            .db()
            .batch_scan_keys(&table_key_prefix_tuples, each_limit)
            .unwrap()
            .into_iter()
            .map(|(t, k)| (t.to_string(), blob_to_string(k.as_ref())))
            .collect();
        Json(res)
    }

    #[mu_function]
    fn batch_delete<'a>(ctx: &'a mut MuContext, req: Json<Vec<(String, String)>>) {
        let req = req.into_inner();
        let table_key_tuples = req
            .iter()
            .map(|(x, y)| (x.as_str(), y.as_bytes()))
            .collect::<Vec<_>>();
        ctx.db().batch_delete(&table_key_tuples).unwrap()
    }
}
