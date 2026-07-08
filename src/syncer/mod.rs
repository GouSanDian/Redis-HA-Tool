//! syncer/mod.rs - 同步器模块入口
//!
//! 本模块实现核心同步引擎，包括 Input/Output 管道和 Syncer 状态机。

pub mod state_machine;
pub mod transaction;
pub mod syncer;
pub mod input;
pub mod output;
pub mod channel;

pub use state_machine::*;
pub use transaction::*;
pub use syncer::*;
pub use input::*;
pub use output::*;
pub use channel::*;

use async_trait::async_trait;
use tokio::sync::Notify;
use std::sync::Arc;
use crate::error::Result;
use crate::store::Reader;

/// Syncer 状态枚举
///
/// 定义同步器的生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncState {
    /// 准备运行
    ReadyRun,
    /// 正在运行
    Run,
    /// 已暂停
    Pause,
    /// 已停止
    Stop,
}

impl Default for SyncState {
    fn default() -> Self {
        SyncState::ReadyRun
    }
}

impl SyncState {
    /// 是否可运行
    pub fn can_run(&self) -> bool {
        matches!(self, SyncState::ReadyRun | SyncState::Pause)
    }
    
    /// 是否正在运行
    pub fn is_running(&self) -> bool {
        matches!(self, SyncState::Run)
    }
    
    /// 是否已停止
    pub fn is_stopped(&self) -> bool {
        matches!(self, SyncState::Stop)
    }
}

/// Syncer 角色枚举
///
/// 定义同步器在高可用架构中的角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncRole {
    /// Leader（主同步器）
    Leader,
    /// Follower（从同步器）
    Follower,
}

impl Default for SyncRole {
    fn default() -> Self {
        SyncRole::Leader
    }
}

/// Input trait - 数据输入接口
///
/// 从源 Redis 读取数据的接口。
#[async_trait]
pub trait Input: Send + Sync {
    /// 运行输入器
    async fn run(&self) -> Result<()>;
    
    /// 停止输入器
    async fn stop(&self) -> Result<()>;
}

/// Output trait - 数据输出接口
///
/// 向目标 Redis 写入数据的接口。
#[async_trait]
pub trait Output: Send + Sync {
    /// 发送数据到目标 Redis
    ///
    /// # 参数
    /// - reader: 数据读取器（RDB 或 AOF）
    ///
    /// # 返回
    /// - 发送的字节数
    async fn send(&self, reader: Box<dyn Reader>) -> Result<i64>;
    
    /// 停止输出器
    async fn stop(&self) -> Result<()>;
    
    /// 创建新的已认证的 TCP 连接
    async fn new_stream(&self) -> Result<tokio::net::TcpStream>;
    
    /// 通过已有的连接发送数据
    ///
    /// # 参数
    /// - reader: 数据读取器（RDB 或 AOF）
    /// - stream: 已有的 TCP 连接
    ///
    /// # 返回
    /// - 发送的字节数
    async fn send_with_stream(&self, reader: Box<dyn Reader>, stream: &mut tokio::net::TcpStream) -> Result<i64>;
}

/// Channel trait - 数据通道接口
///
/// 桥接 Input 写入器和 Output 读取器。
#[async_trait]
pub trait Channel: Send + Sync {
    /// 获取数据读取器
    ///
    /// # 参数
    /// - offset: 数据起始偏移量
    async fn get_reader(&self, offset: i64) -> Result<Box<dyn Reader>>;
    
    /// 检查从指定 offset 开始有多少可用数据
    ///
    /// # 参数
    /// - offset: 全局数据偏移量
    ///
    /// # 返回
    /// 可用字节数，0 表示暂无新数据
    async fn available_bytes(&self, offset: i64) -> Result<i64>;
    
    /// 获取 RDB 写入器
    ///
    /// # 参数
    /// - run_id: Redis 运行 ID
    /// - offset: RDB 数据起始偏移量
    /// - size: RDB 数据大小
    async fn get_rdb_writer(
        &self,
        run_id: &str,
        offset: i64,
        size: i64,
    ) -> Result<Box<dyn tokio::io::AsyncWrite + Send + Unpin>>;
    
    /// 获取 AOF 写入器
    ///
    /// # 参数
    /// - run_id: Redis 运行 ID
    /// - offset: AOF 数据起始偏移量
    async fn get_aof_writer(
        &self,
        run_id: &str,
        offset: i64,
    ) -> Result<Box<dyn tokio::io::AsyncWrite + Send + Unpin>>;

    /// 获取数据通知器
    ///
    /// Writer 写入数据后会通知等待者，避免轮询。
    fn data_notify(&self) -> Arc<Notify>;
}

/// Syncer trait - 同步器接口
///
/// 管理同步任务的生命周期和状态机。
#[async_trait]
pub trait Syncer: Send + Sync {
    /// 运行同步器
    async fn run(&self) -> Result<()>;
    
    /// 停止同步器
    async fn stop(&self) -> Result<()>;
    
    /// 暂停同步器
    async fn pause(&self) -> Result<()>;
    
    /// 恢复同步器
    async fn resume(&self) -> Result<()>;
    
    /// 获取当前状态
    fn status(&self) -> SyncState;
    
    /// 获取当前角色
    fn role(&self) -> SyncRole;
}