/// error.rs - 全局错误类型定义
/// 
/// 本文件定义了整个 redis-ha-tool 项目使用的错误类型，
/// 基于 thiserror 库实现，提供清晰的错误信息。

use thiserror::Error;

/// 同步错误枚举
/// 
/// 定义了同步过程中可能出现的各种错误类型。
#[derive(Debug, Error)]
pub enum SyncError {
    /// 需要重启或退出的错误
    #[error("同步器需要重启或退出: {0}")]
    Break(String),

    /// 角色变化错误（Leader/Follower 切换）
    #[error("角色变化检测: {0}")]
    Role(String),

    /// 数据损坏错误
    #[error("数据损坏: {0}")]
    Corrupted(String),

    /// 用户手动停止同步
    #[error("同步被用户停止")]
    StopSync,

    /// Redis 拓扑变化错误
    #[error("Redis 拓扑变化: {0}")]
    RedisTopologyChanged(String),

    /// 配置错误
    #[error("配置错误: {0}")]
    Config(String),

    /// 协议错误（RESP 解析等）
    #[error("协议错误: {0}")]
    Protocol(String),

    /// IO 错误
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Redis 客户端错误
    #[error(transparent)]
    Redis(#[from] redis::RedisError),

    /// Tokio Task Join 错误
    #[error("Tokio Task 错误: {0}")]
    JoinError(String),

    /// 其他错误（使用 anyhow）
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<tokio::task::JoinError> for SyncError {
    fn from(err: tokio::task::JoinError) -> Self {
        SyncError::JoinError(err.to_string())
    }
}

/// 全局 Result 类型别名
/// 
/// 使用 SyncError 作为默认错误类型，简化函数签名。
pub type Result<T> = std::result::Result<T, SyncError>;

/// RESP 协议解析错误
/// 
/// 定义 RESP 协议解析过程中的具体错误类型。
#[derive(Debug, Error)]
pub enum RespError {
    /// 不完整的 RESP 数据（需要更多数据）
    #[error("RESP 数据不完整")]
    Incomplete,

    /// 无效的 RESP 类型标记
    #[error("无效的 RESP 类型标记: {0}")]
    InvalidType(char),

    /// 无效的整数格式
    #[error("无效的整数格式: {0}")]
    InvalidInteger(String),

    /// 无效的长度
    #[error("无效的长度: {0}")]
    InvalidLength(i64),

    /// 数据超出缓冲区限制
    #[error("数据超出缓冲区限制: 最大 {max}，实际 {actual}")]
    BufferOverflow { max: usize, actual: usize },

    /// IO 错误
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// RESP Result 类型别名
pub type RespResult<T> = std::result::Result<T, RespError>;

/// 配置错误
/// 
/// 定义配置加载和验证过程中的错误类型。
#[derive(Debug, Error)]
pub enum ConfigError {
    /// 配置文件不存在
    #[error("配置文件不存在: {0}")]
    FileNotFound(String),

    /// 配置文件读取失败
    #[error("配置文件读取失败: {0}")]
    ReadError(String),

    /// 配置格式错误
    #[error("配置格式错误: {0}")]
    FormatError(String),

    /// 配置验证失败
    #[error("配置验证失败: {0}")]
    ValidationError(String),

    /// IO 错误
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// 配置 Result 类型别名
pub type ConfigResult<T> = std::result::Result<T, ConfigError>;

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;

    /// 测试 SyncError 的错误消息格式
    #[test]
    fn test_sync_error_messages() {
        let err = SyncError::Break("测试原因".to_string());
        assert_eq!(err.to_string(), "同步器需要重启或退出: 测试原因");

        let err = SyncError::StopSync;
        assert_eq!(err.to_string(), "同步被用户停止");
    }

    /// 测试 RespError 的错误消息格式
    #[test]
    fn test_resp_error_messages() {
        let err = RespError::InvalidType('?');
        assert_eq!(err.to_string(), "无效的 RESP 类型标记: ?");

        let err = RespError::BufferOverflow { max: 100, actual: 200 };
        assert_eq!(err.to_string(), "数据超出缓冲区限制: 最大 100，实际 200");
    }

    /// 测试 ConfigError 的错误消息格式
    #[test]
    fn test_config_error_messages() {
        let err = ConfigError::FileNotFound("/path/to/config.json".to_string());
        assert_eq!(err.to_string(), "配置文件不存在: /path/to/config.json");

        let err = ConfigError::ValidationError("Redis 地址不能为空".to_string());
        assert_eq!(err.to_string(), "配置验证失败: Redis 地址不能为空");
    }

    /// 测试 IO 错误自动转换
    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "文件不存在");
        let sync_err: SyncError = io_err.into();
        assert!(matches!(sync_err, SyncError::Io(_)));
    }
}