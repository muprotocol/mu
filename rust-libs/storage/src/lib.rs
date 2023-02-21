use anyhow::{bail, Error, Result};
use async_trait::async_trait;
use dyn_clonable::clonable;
use mu_stack::StackID;
use pin_project_lite::pin_project;
use s3::{creds::Credentials, Bucket};
use serde::Deserialize;
use std::{fmt::Debug, pin::Pin};
use storage_embedded_juicefs::{InternalStorageConfig, JuicefsRunner, LiveStorageConfig};
use tokio::io::{AsyncRead, AsyncWrite};

pub struct Object {
    pub key: String,
    pub size: u64,
}

#[async_trait]
#[clonable]
pub trait StorageClient: Send + Sync + Debug + Clone {
    async fn get(
        &self,
        stack_id: StackID,
        storage_name: &str,
        key: &str,
        writer: &mut (dyn AsyncWrite + Send + Sync + Unpin),
    ) -> Result<u16>;

    async fn put(
        &self,
        stack_id: StackID,
        storage_name: &str,
        key: &str,
        reader: &mut (dyn AsyncRead + Send + Sync + Unpin),
    ) -> Result<u16>;

    async fn delete(&self, stack_id: StackID, storage_name: &str, key: &str) -> Result<u16>;

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
    external: Option<LiveStorageConfig>,
    internal: Option<InternalStorageConfig>,
}

#[async_trait]
#[clonable]
pub trait StorageManager: Send + Sync + Clone {
    fn make_client(&self) -> anyhow::Result<Box<dyn StorageClient>>;
    async fn stop(self) -> anyhow::Result<()>;
}

#[derive(Clone)]
struct StorageManagerImpl {
    inner: Option<Box<dyn JuicefsRunner>>,
    config: LiveStorageConfig,
}

#[async_trait]
impl StorageManager for StorageManagerImpl {
    fn make_client(&self) -> anyhow::Result<Box<dyn StorageClient>> {
        Ok(Box::new(StorageClientImpl::new(&self.config)?))
    }

    async fn stop(self) -> anyhow::Result<()> {
        match self.inner {
            Some(r) => r.stop().await,
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

        let bucket = Bucket::new(&config.bucket_name, region, credentials)?;

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
}

#[async_trait]
impl StorageClient for StorageClientImpl {
    async fn get(
        &self,
        stack_id: StackID,
        storage_name: &str,
        key: &str,
        writer: &mut (dyn AsyncWrite + Send + Sync + Unpin),
    ) -> Result<u16> {
        let mut wrapper = AsyncWriterWrapper { writer };
        let path = Self::create_path(stack_id, storage_name, key);
        self.bucket
            .get_object_stream(path, &mut wrapper)
            .await
            .map_err(|e| e.into())
    }

    async fn put(
        &self,
        stack_id: StackID,
        storage_name: &str,
        key: &str,
        reader: &mut (dyn AsyncRead + Send + Sync + Unpin),
    ) -> Result<u16> {
        let mut wrapper = AsyncReaderWrapper { reader };
        let path = Self::create_path(stack_id, storage_name, key);

        self.bucket
            .put_object_stream(&mut wrapper, path)
            .await
            .map_err(|e| e.into())
    }

    async fn delete(&self, stack_id: StackID, storage_name: &str, key: &str) -> Result<u16> {
        let path = Self::create_path(stack_id, storage_name, key);

        let resp = self.bucket.delete_object(path).await?;

        Ok(resp.status_code())
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
