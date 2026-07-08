//! cmd/syncer_cmd.rs - SyncerCmd 实现
//!
//! 本文件实现 Cmd trait 和 SyncerCmd。

use async_trait::async_trait;
use crate::error::Result;

/// Cmd trait - 命令接口
#[async_trait]
pub trait Cmd: Send + Sync {
    /// 命令名称
    fn name(&self) -> &str;
    
    /// 启动命令
    async fn start(&self) -> Result<()>;
    
    /// 停止命令
    async fn stop(&self) -> Result<()>;
}

/// SyncerCmd - 同步器命令
///
/// 封装 Syncer，提供命令行入口。
pub struct SyncerCmd {
    /// 命令名称
    name: String,
}

impl SyncerCmd {
    /// 创建 SyncerCmd
    pub fn new(name: String) -> Self {
        SyncerCmd { name }
    }
}

#[async_trait]
impl Cmd for SyncerCmd {
    fn name(&self) -> &str {
        &self.name
    }
    
    async fn start(&self) -> Result<()> {
        tracing::info!("启动 SyncerCmd: {}", self.name);
        Ok(())
    }
    
    async fn stop(&self) -> Result<()> {
        tracing::info!("停止 SyncerCmd: {}", self.name);
        Ok(())
    }
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;
    
    /// 测试 SyncerCmd 创建
    #[test]
    fn test_syncer_cmd_create() {
        let cmd = SyncerCmd::new("sync".to_string());
        assert_eq!(cmd.name(), "sync");
    }
    
    /// 测试 SyncerCmd 启动和停止
    #[tokio::test]
    async fn test_syncer_cmd_start_stop() {
        let cmd = SyncerCmd::new("test".to_string());
        
        cmd.start().await.unwrap();
        cmd.stop().await.unwrap();
    }
}