/// constants.rs - 配置系统常量定义
/// 
/// 本文件定义了 redis-ha-tool 项目中使用的各种常量，
/// 包括 Checkpoint 键名、Circle Key 前缀、延迟测量键等。

/// Checkpoint 存储键名
/// 
/// 用于在目标 Redis 的 DB 0 中存储 checkpoint 信息。
pub const CHECKPOINT_KEY: &str = "redis_ha_tool_checkpoint";

/// Checkpoint Hash 映射键名
/// 
/// 用于存储 runId → checkpointName 的映射关系。
pub const CHECKPOINT_KEY_HASH_KEY: &str = "redis_ha_tool_checkpoint_hash";

/// Circle Key 前缀
/// 
/// 用于双向同步时防止循环复制。
/// 格式: redis_ha_tool_circle_{md5_hash}
pub const CIRCLE_PREFIX_KEY: &str = "redis_ha_tool_circle_";

/// 延迟测量键前缀
/// 
/// 用于测量同步延迟，格式: redis_ha_tool_delay_{timestamp}
pub const DELAY_PREFIX_KEY: &str = "redis_ha_tool_delay_";

/// Leader 选举键前缀
/// 
/// 用于 Leader-Follower 高可用选举。
pub const ELECTION_PREFIX_KEY: &str = "redis_ha_tool_input_election_";

/// 默认 HTTP 服务端口
pub const DEFAULT_HTTP_PORT: u16 = 8080;

/// 默认 gRPC 服务端口
pub const DEFAULT_GRPC_PORT: u16 = 9090;

/// 默认配置文件路径
pub const DEFAULT_CONFIG_FILE: &str = "config/config.json";

/// 默认存储目录
pub const DEFAULT_STORE_DIR: &str = "/tmp/redis-ha-tool";

/// 默认 RDB 并行回放数
pub const DEFAULT_RDB_PARALLEL: usize = 4;

/// 默认批量大小（字节）
pub const DEFAULT_BATCH_SIZE: usize = 64 * 1024; // 64KB

/// 默认批量计数
pub const DEFAULT_BATCH_COUNT: usize = 100;

/// 默认 AOF 日志大小限制（字节）
pub const DEFAULT_LOG_SIZE: usize = 100 * 1024 * 1024; // 100MB

/// 默认 AOF 文件头部大小（字节）
pub const DEFAULT_HEADER_SIZE: usize = 16;

/// 默认连接超时（秒）
pub const DEFAULT_CONNECT_TIMEOUT: u64 = 5;

/// 默认读写超时（秒）
pub const DEFAULT_READ_WRITE_TIMEOUT: u64 = 30;

/// 默认心跳间隔（秒）
pub const DEFAULT_KEEPALIVE_INTERVAL: u64 = 10;

/// 默认 ACK 发送间隔（毫秒）
pub const DEFAULT_ACK_INTERVAL_MS: u64 = 1000;

/// 默认 Checkpoint 更新间隔（毫秒）
pub const DEFAULT_CHECKPOINT_INTERVAL_MS: u64 = 5000;

/// 默认日志级别
pub const DEFAULT_LOG_LEVEL: &str = "info";

/// 默认日志保留天数
pub const DEFAULT_LOG_MAX_AGE: usize = 7;

/// 默认日志文件最大数量
pub const DEFAULT_LOG_MAX_FILES: usize = 10;

/// 默认单日志文件大小（字节）
pub const DEFAULT_LOG_MAX_SIZE: usize = 100 * 1024 * 1024; // 100MB

/// 默认最大存储大小（字节）
pub const DEFAULT_MAX_STORAGE_SIZE: usize = 1024 * 1024 * 1024; // 1GB

/// 永不转发的命令列表（NoRoute 命令）
/// 
/// 这些命令不会从源端同步到目标端。
pub const NO_ROUTE_COMMANDS: &[&str] = &[
    "AUTH",
    "CLUSTER",
    "COMMAND",
    "CONFIG",
    "DEBUG",
    "FLUSHALL",
    "FLUSHDB",
    "INFO",
    "KEYS",
    "MONITOR",
    "PSYNC",
    "REPLCONF",
    "SYNC",
    "SHUTDOWN",
    "SLAVEOF",
    "SCRIPT",
    "SENTINEL",
];

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;

    /// 测试常量值不为空
    #[test]
    fn test_constants_not_empty() {
        assert!(!CHECKPOINT_KEY.is_empty());
        assert!(!CHECKPOINT_KEY_HASH_KEY.is_empty());
        assert!(!CIRCLE_PREFIX_KEY.is_empty());
        assert!(!DELAY_PREFIX_KEY.is_empty());
        assert!(!ELECTION_PREFIX_KEY.is_empty());
    }

    /// 测试常量前缀格式正确
    #[test]
    fn test_prefix_format() {
        assert!(CIRCLE_PREFIX_KEY.ends_with('_'));
        assert!(DELAY_PREFIX_KEY.ends_with('_'));
        assert!(ELECTION_PREFIX_KEY.ends_with('_'));
    }

    /// 测试默认值合理
    #[test]
    fn test_default_values_reasonable() {
        assert!(DEFAULT_RDB_PARALLEL >= 1);
        assert!(DEFAULT_BATCH_SIZE > 0);
        assert!(DEFAULT_BATCH_COUNT > 0);
        assert!(DEFAULT_LOG_SIZE > 0);
        assert!(DEFAULT_CONNECT_TIMEOUT > 0);
    }

    /// 测试 NoRoute 命令列表包含必要命令
    #[test]
    fn test_no_route_commands() {
        assert!(NO_ROUTE_COMMANDS.contains(&"AUTH"));
        assert!(NO_ROUTE_COMMANDS.contains(&"PSYNC"));
        assert!(NO_ROUTE_COMMANDS.contains(&"FLUSHALL"));
        assert!(NO_ROUTE_COMMANDS.contains(&"FLUSHDB"));
        assert!(NO_ROUTE_COMMANDS.contains(&"INFO"));
    }
}