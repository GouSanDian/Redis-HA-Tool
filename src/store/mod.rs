//! store/mod.rs - 存储系统模块入口
//!
//! 本模块实现本地文件存储引擎，管理 RDB 和 AOF 文件的生命周期。

pub mod dataset;
pub mod storer;
pub mod reader;
pub mod writer;

pub use dataset::*;
pub use storer::*;
pub use reader::*;
pub use writer::*;

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite};
use tokio::sync::Notify;
use std::sync::Arc;
use crate::error::Result;

/// Reader 类型枚举
///
/// 区分 RDB 和 AOF 两种读取器类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderType {
    /// RDB 文件读取器
    Rdb,
    /// AOF 文件读取器
    Aof,
}

/// Reader trait - 数据读取接口
///
/// 提供统一的读取接口，支持 RDB 和 AOF 两种数据源。
#[async_trait]
pub trait Reader: AsyncRead + AsyncSeek + Send + Unpin {
    /// 获取 Reader 类型
    fn reader_type(&self) -> ReaderType;
    
    /// 获取当前读取偏移量
    fn offset(&self) -> i64;
    
    /// 获取数据大小（仅 RDB 有效）
    fn size(&self) -> Option<i64>;
}

/// Storer trait - 存储管理器接口
///
/// 管理本地文件存储，按 runId 组织目录结构。
#[async_trait]
pub trait Storer: Send + Sync {
    /// 获取指定 offset 的 Reader
    ///
    /// # 参数
    /// - offset: 数据起始偏移量
    ///
    /// # 返回
    /// Reader 对象，根据 offset 自动判断 RDB 或 AOF 类型
    async fn get_reader(&self, offset: i64) -> Result<Box<dyn Reader>>;
    
    /// 获取 RDB Writer
    ///
    /// # 参数
    /// - run_id: Redis 运行 ID
    /// - offset: RDB 数据起始偏移量
    /// - size: RDB 数据大小
    ///
    /// # 返回
    /// 异步 Writer 对象
    async fn get_rdb_writer(
        &self,
        run_id: &str,
        offset: i64,
        size: i64,
    ) -> Result<Box<dyn AsyncWrite + Send + Unpin>>;
    
    /// 获取 AOF Writer
    ///
    /// # 参数
    /// - run_id: Redis 运行 ID
    /// - offset: AOF 数据起始偏移量
    ///
    /// # 返回
    /// 异步 Writer 对象
    async fn get_aof_writer(
        &self,
        run_id: &str,
        offset: i64,
    ) -> Result<Box<dyn AsyncWrite + Send + Unpin>>;
    
    /// 初始化数据集
    ///
    /// 扫描存储目录，重建内存状态。
    async fn init_data_set(&self) -> Result<()>;
    
    /// 执行垃圾回收
    ///
    /// 清理超出大小限制的旧文件。
    async fn gc_data_set(&self) -> Result<()>;
    
    /// 验证 run_id 是否存在
    ///
    /// # 参数
    /// - run_id: Redis 运行 ID
    ///
    /// # 返回
    /// true 表示存在，false 表示不存在
    fn verify_run_id(&self, run_id: &str) -> bool;
    
    /// 检查从指定全局 offset 开始有多少可用数据
    ///
    /// # 参数
    /// - offset: 全局数据偏移量
    ///
    /// # 返回
    /// 可用字节数，0 表示暂无新数据
    async fn available_bytes(&self, offset: i64) -> Result<i64>;

    /// 获取数据通知器
    ///
    /// Writer 写入数据后会通知等待者，避免轮询。
    fn data_notify(&self) -> Arc<Notify> {
        Arc::new(Notify::new())
    }
}