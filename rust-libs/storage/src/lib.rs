use anyhow::{bail, Error, Result};
use async_trait::async_trait;
use dyn_clonable::clonable;
use mu_stack::StackID;
use pin_project_lite::pin_project;
use s3::{creds::Credentials, Bucket};
use serde::Deserialize;
use std::{fmt::Debug, ops::Deref, pin::Pin};
use storage_embedded_juicefs::{InternalStorageConfig, JuicefsRunner, LiveStorageConfig};
use tokio::io::{AsyncRead, AsyncWrite};

const METADATA_PREFIX: &str = "!";

pub struct Object {
    pub key: String,
    pub size: u64,
}

#[async_trait]
#[clonable]
pub trait StorageClient: Send + Sync + Clone {
    async fn update_stack_storages(
        &self,
        stack_id: StackID,
        storage_delete_pairs: Vec<(&str, DeleteStorage)>,
    ) -> Result<()>;

    async fn storage_list(&self, stack_id: StackID) -> Result<Vec<String>>;

    async fn contains_storage(&self, stack_id: StackID, storage_name: &str) -> Result<bool>;

    async fn remove_storage(&self, stack_id: StackID, storage_name: &str) -> Result<()>;

    async fn get(
        &self,
        stack_id: StackID,
        storage_name: &str,
        key: &str,
        writer: &mut (dyn AsyncWrite + Send + Sync + Unpin),
    ) -> Result<()>;

    async fn put(
        &self,
        stack_id: StackID,
        storage_name: &str,
        key: &str,
        reader: &mut (dyn AsyncRead + Send + Sync + Unpin),
    ) -> Result<()>;

    async fn delete(&self, stack_id: StackID, storage_name: &str, key: &str) -> Result<()>;

    async fn list(
        &self,
        stack_id: StackID,
        storage_name: &str,
        prefix: &str,
    ) -> Result<Vec<Object>>;
}

#[derive(Clone, Debug)]
struct StorageClientImpl {
    bucket: Bucket,
}

// exactly one should be provided
// used struct instead of enum only for better representation in config file
#[derive(Deserialize)]
pub struct StorageConfig {
    pub external: Option<LiveStorageConfig>,
    pub internal: Option<InternalStorageConfig>,
}

#[async_trait]
#[clonable]
pub trait StorageManager: Send + Sync + Clone {
    fn make_client(&self) -> anyhow::Result<Box<dyn StorageClient>>;
    async fn stop(&self) -> anyhow::Result<()>;
}

#[derive(Clone)]
struct StorageManagerImpl {
    inner: Option<Box<dyn JuicefsRunner>>,
    config: LiveStorageConfig,
}

#[async_trait]
impl StorageManager for StorageManagerImpl {
    fn make_client(&self) -> anyhow::Result<Box<dyn StorageClient>> {
        // TODO check healthy
        Ok(Box::new(StorageClientImpl::new(&self.config)?))
    }

    async fn stop(&self) -> anyhow::Result<()> {
        match self.inner {
            Some(ref r) => r.stop().await,
            None => Ok(()),
        }
    }
}

impl StorageClientImpl {
    pub fn new(config: &LiveStorageConfig) -> Result<StorageClientImpl> {
        let credentials = Credentials::new(
            config.auth_config.access_key.as_deref(),
            config.auth_config.secret_key.as_deref(),
            config.auth_config.security_token.as_deref(),
            config.auth_config.session_token.as_deref(),
            config.auth_config.profile.as_deref(),
        )
        .map_err(|e| Error::msg(e.to_string()))?;

        let region = s3::Region::Custom {
            region: config.region.region.to_owned(),
            endpoint: config.region.endpoint.clone(),
        };

        let mut bucket = Bucket::new(&config.bucket_name, region, credentials)?;
        bucket.set_path_style();

        Ok(StorageClientImpl { bucket })
    }

    fn create_path(stack_id: StackID, storage_name: &str, key: &str) -> String {
        format!("{stack_id}/{storage_name}/{key}")
    }

    fn create_object(object: &s3::serde_types::Object) -> Object {
        let key = object
            .key
            .match_indices('/')
            .nth(1)
            .map(|(i, _)| object.key.split_at(i).1.to_string());

        // TODO: deserialize last modified date
        Object {
            key: key.unwrap_or_default(),
            size: object.size,
        }
    }

    async fn add_storage(&self, stack_id: StackID, name: &str) -> Result<()> {
        let path = format!("{METADATA_PREFIX}/{stack_id}/{name}");
        self.bucket.put_object_stream(&mut &b""[..], path).await?;
        Ok(())
    }
}

#[async_trait]
impl StorageClient for StorageClientImpl {
    async fn update_stack_storages(
        &self,
        stack_id: StackID,
        storage_delete_pairs: Vec<(&str, DeleteStorage)>,
    ) -> Result<()> {
        let existing_storages = self.storage_list(stack_id).await?;

        for (storage_name, is_delete) in storage_delete_pairs {
            let storage_name = storage_name.to_string();
            if !existing_storages.contains(&storage_name) && !*is_delete {
                self.add_storage(stack_id, &storage_name).await?;
            } else if existing_storages.contains(&storage_name) && *is_delete {
                self.remove_storage(stack_id, &storage_name).await?;
            }
        }

        Ok(())
    }

    async fn storage_list(&self, stack_id: StackID) -> Result<Vec<String>> {
        let prefix = format!("{METADATA_PREFIX}/{stack_id}/");

        let resp = self.bucket.list(prefix, None).await?;

        let objects = resp[0]
            .contents
            .iter()
            .filter_map(|x| x.key.split('/').last().map(ToString::to_string))
            .collect();

        Ok(objects)
    }

    async fn contains_storage(&self, stack_id: StackID, storage_name: &str) -> Result<bool> {
        Ok(self
            .storage_list(stack_id)
            .await?
            .contains(&storage_name.into()))
    }

    async fn remove_storage(&self, stack_id: StackID, storage_name: &str) -> Result<()> {
        // remove from manifest
        let path = format!("{METADATA_PREFIX}/{stack_id}/{storage_name}");
        self.bucket.delete_object(path).await?;

        // remove data
        let keys = self
            .list(stack_id, storage_name, "")
            .await?
            .into_iter()
            .map(|o| o.key);

        for key in keys {
            let path = Self::create_path(stack_id, storage_name, &key);
            self.bucket.delete_object(path).await?;
        }

        Ok(())
    }

    async fn get(
        &self,
        stack_id: StackID,
        storage_name: &str,
        key: &str,
        writer: &mut (dyn AsyncWrite + Send + Sync + Unpin),
    ) -> Result<()> {
        let mut wrapper = AsyncWriterWrapper { writer };
        let path = Self::create_path(stack_id, storage_name, key);
        self.bucket.get_object_stream(path, &mut wrapper).await?;
        Ok(())
    }

    async fn put(
        &self,
        stack_id: StackID,
        storage_name: &str,
        key: &str,
        reader: &mut (dyn AsyncRead + Send + Sync + Unpin),
    ) -> Result<()> {
        if !self.contains_storage(stack_id, storage_name).await? {
            self.add_storage(stack_id, storage_name).await?
        }
        let mut wrapper = AsyncReaderWrapper { reader };
        let path = Self::create_path(stack_id, storage_name, key);

        self.bucket.put_object_stream(&mut wrapper, path).await?;
        Ok(())
    }

    async fn delete(&self, stack_id: StackID, storage_name: &str, key: &str) -> Result<()> {
        let path = Self::create_path(stack_id, storage_name, key);

        self.bucket.delete_object(path).await?;

        Ok(())
    }

    async fn list(
        &self,
        stack_id: StackID,
        storage_name: &str,
        prefix: &str,
    ) -> Result<Vec<Object>> {
        let prefix = Self::create_path(stack_id, storage_name, prefix);

        let resp = self.bucket.list(prefix, None).await?;

        let objects = resp[0]
            .contents
            .iter()
            .map(StorageClientImpl::create_object)
            .collect::<Vec<_>>();

        Ok(objects)
    }
}

pub async fn start(config: &StorageConfig) -> Result<Box<dyn StorageManager>> {
    let (inner, config) = match (&config.external, &config.internal) {
        (Some(ext_config), None) => (None, ext_config.clone()),
        (None, Some(int_config)) => {
            let (runner, config) = storage_embedded_juicefs::start(int_config).await?;
            (Some(runner), config)
        }
        _ => bail!("Exactly one of internal or external storage config should be provided"),
    };

    Ok(Box::new(StorageManagerImpl { inner, config }))
}

pin_project! {
    struct AsyncReaderWrapper<'a> {
        reader: &'a mut (dyn AsyncRead + Send + Sync + Unpin)
    }
}

impl<'a> AsyncRead for AsyncReaderWrapper<'a> {
    fn poll_read(
        self: std::pin::Pin<&mut AsyncReaderWrapper<'a>>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(self.project().reader).poll_read(cx, buf)
    }
}

pin_project! {
    struct AsyncWriterWrapper<'a>{
        writer: &'a mut (dyn AsyncWrite + Send + Sync + Unpin)
    }
}

impl<'a> AsyncWrite for AsyncWriterWrapper<'a> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::result::Result<usize, std::io::Error>> {
        Pin::new(self.project().writer).poll_write(cx, buf)
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::result::Result<(), std::io::Error>> {
        Pin::new(self.project().writer).poll_flush(cx)
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::result::Result<(), std::io::Error>> {
        Pin::new(self.project().writer).poll_shutdown(cx)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteStorage(pub bool);

impl Deref for DeleteStorage {
    type Target = bool;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(test)]
mod test {
    use mu_common::serde_support::{IpOrHostname, TcpPortAddress};
    use storage_embedded_juicefs::StorageInfo;

    use super::*;

    const STACK_ID: StackID = StackID::SolanaPublicKey([1; 32]);

    async fn test_start() -> Result<Box<dyn StorageManager>> {
        let storage_info = StorageInfo {
            endpoint: TcpPortAddress {
                address: IpOrHostname::Ip("127.0.0.1".parse().unwrap()),
                port: 9015,
            },
        };
        let internal_conf = InternalStorageConfig {
            metadata_tikv_endpoints: vec![],
            object_storage_tikv_endpoints: vec![],
            storage: storage_info,
        };
        let conf = StorageConfig {
            external: None,
            internal: Some(internal_conf),
        };
        start(&conf).await
    }

    #[tokio::test]
    #[ignore = "TODO"]
    async fn create_update_delete_manifest() {
        let manager = test_start().await.unwrap();
        let client = manager.make_client().unwrap();

        let insertion_storages = vec!["s1", "s2", "s3", "s4"];

        let stor_del_pairs = insertion_storages
            .clone()
            .into_iter()
            .map(|x| (x, DeleteStorage(false)))
            .collect::<Vec<_>>();

        client
            .update_stack_storages(STACK_ID, stor_del_pairs)
            .await
            .unwrap();

        let x = client.storage_list(STACK_ID).await.unwrap();

        assert_eq!(insertion_storages, x);
    }
}
