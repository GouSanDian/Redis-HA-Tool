//! syncer/channel.rs - 数据通道实现
//!
//! 本文件实现 FileChannel，基于文件存储的数据通道。

use async_trait::async_trait;
use std::sync::Arc;
use tokio::io::AsyncWrite;
use tokio::sync::Notify;
use crate::error::Result;
use crate::syncer::Channel;
use crate::store::{Reader, Storer};

pub struct FileChannel {
    storer: Arc<dyn Storer>,
    notify: Arc<Notify>,
}

impl FileChannel {
    pub fn new(storer: Arc<dyn Storer>) -> Self {
        let notify = storer.data_notify();
        FileChannel { storer, notify }
    }
}

#[async_trait]
impl Channel for FileChannel {
    async fn get_reader(&self, offset: i64) -> Result<Box<dyn Reader>> {
        self.storer.get_reader(offset).await
    }

    async fn available_bytes(&self, offset: i64) -> Result<i64> {
        self.storer.available_bytes(offset).await
    }

    async fn get_rdb_writer(
        &self,
        run_id: &str,
        offset: i64,
        size: i64,
    ) -> Result<Box<dyn AsyncWrite + Send + Unpin>> {
        self.storer.get_rdb_writer(run_id, offset, size).await
    }

    async fn get_aof_writer(
        &self,
        run_id: &str,
        offset: i64,
    ) -> Result<Box<dyn AsyncWrite + Send + Unpin>> {
        self.storer.get_aof_writer(run_id, offset).await
    }

    fn data_notify(&self) -> Arc<Notify> {
        self.notify.clone()
    }
}