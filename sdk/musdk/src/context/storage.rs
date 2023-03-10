use std::borrow::Cow;

use musdk_common::{
    incoming_message::{storage::Object, IncomingMessage as IM},
    outgoing_message::{storage::*, OutgoingMessage as OM},
};

use crate::{Error, Result};

pub struct StorageHandle<'a> {
    pub(super) context: &'a mut super::MuContext,
}

impl<'a> StorageHandle<'a> {
    fn request(&mut self, req: OM) -> Result<IM<'static>> {
        self.context.write_message(req)?;
        self.context.read_message()
    }

    pub fn delete(&mut self, storage_name: &str, key: &str) -> Result<()> {
        let req = StorageDelete {
            storage_name: Cow::Borrowed(storage_name),
            key: Cow::Borrowed(key),
        };

        let resp = self.request(OM::StorageDelete(req))?;
        from_empty_resp(resp, "StorageDelete")
    }

    pub fn search_by_prefix(&mut self, storage_name: &str, prefix: &str) -> Result<Vec<Object>> {
        let req = StorageList {
            storage_name: Cow::Borrowed(storage_name),
            prefix: Cow::Borrowed(prefix),
        };

        let resp = self.request(OM::StorageList(req))?;
        match resp {
            IM::ObjectListResult(x) => Ok(x.list),
            resp => resp_to_err(resp, "StorageList"),
        }
    }

    pub fn get(&mut self, storage_name: &str, key: &str) -> Result<Cow<[u8]>> {
        let req = StorageGet {
            storage_name: Cow::Borrowed(storage_name),
            key: Cow::Borrowed(key),
        };

        let resp = self.request(OM::StorageGet(req))?;

        match resp {
            IM::StorageGetResult(x) => Ok(x.data),
            resp => resp_to_err(resp, "StorageGet"),
        }
    }

    pub fn put(&mut self, storage_name: &str, key: &str, data: &[u8]) -> Result<()> {
        let req = StoragePut {
            storage_name: Cow::Borrowed(storage_name),
            key: Cow::Borrowed(key),
            reader: Cow::Borrowed(data),
        };

        let resp = self.request(OM::StoragePut(req))?;

        from_empty_resp(resp, "StoragePut")
    }
}

fn resp_to_err<T>(resp: IM, kind_name: &'static str) -> Result<T> {
    match resp {
        IM::StorageError(e) => Err(Error::StorageError(e.error.into_owned())),
        _ => Err(Error::UnexpectedMessageKind(kind_name)),
    }
}

fn from_empty_resp(resp: IM, kind_name: &'static str) -> Result<()> {
    match resp {
        IM::StorageEmptyResult(_) => Ok(()),
        resp => resp_to_err(resp, kind_name),
    }
}
