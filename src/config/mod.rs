/// config/mod.rs - 配置系统主模块
/// 
/// 本文件定义了 redis-ha-tool 的配置结构体，
/// 以及配置加载和验证方法。

mod constants;

pub use constants::*;

use serde::Deserialize;
use std::path::Path;
use crate::error::{SyncError, Result};

/// Redis 部署类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RedisType {
    /// 单机模式
    Standalone,
    /// Sentinel 高可用模式
    Sentinel,
    /// Cluster 集群模式
    Cluster,
}

impl Default for RedisType {
    fn default() -> Self {
        RedisType::Standalone
    }
}

/// 认证类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthType {
    /// 无认证
    None,
    /// 密码认证
    Password,
    /// ACL 认证
    Acl,
}

impl Default for AuthType {
    fn default() -> Self {
        AuthType::None
    }
}

/// Key 存在时的处理策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyExistsPolicy {
    /// 不处理（保留目标端现有数据）
    None,
    /// 替换为目标端数据
    Replace,
    /// 先 FlushDB 再同步
    Flush,
}

impl Default for KeyExistsPolicy {
    fn default() -> Self {
        KeyExistsPolicy::None
    }
}

/// Slot 范围定义
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct SlotRange {
    /// Slot 起始值（包含）
    pub start: u16,
    /// Slot 结束值（包含）
    pub end: u16,
}

impl SlotRange {
    /// 创建 Slot 范围
    pub fn new(start: u16, end: u16) -> Self {
        SlotRange { start, end }
    }

    /// 检查 Slot 是否在范围内
    pub fn contains(&self, slot: u16) -> bool {
        slot >= self.start && slot <= self.end
    }
}

/// TLS 配置
#[derive(Debug, Clone, Deserialize, Default)]
pub struct TlsConfig {
    /// 是否启用 TLS
    pub enabled: bool,
    /// CA 证书路径
    pub ca_cert: Option<String>,
    /// 客户端证书路径
    pub client_cert: Option<String>,
    /// 客户端私钥路径
    pub client_key: Option<String>,
}

/// 心跳配置
#[derive(Debug, Clone, Deserialize)]
pub struct KeepaliveConfig {
    /// 心跳间隔（秒）
    pub interval: u64,
}

impl Default for KeepaliveConfig {
    fn default() -> Self {
        KeepaliveConfig {
            interval: DEFAULT_KEEPALIVE_INTERVAL,
        }
    }
}

/// Redis 配置
#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    /// Redis 地址列表
    pub addresses: Vec<String>,
    /// 密码
    pub password: Option<String>,
    /// 认证类型
    #[serde(default)]
    pub auth_type: AuthType,
    /// TLS 配置
    #[serde(default)]
    pub tls: TlsConfig,
    /// Redis 部署类型
    #[serde(default)]
    pub redis_type: RedisType,
    /// Slot 范围列表（用于 Cluster 模式过滤）
    pub slots: Option<Vec<SlotRange>>,
    /// 集群分片列表（用于指定同步的分片）
    pub cluster_shards: Option<Vec<String>>,
    /// 心跳配置
    #[serde(default)]
    pub keepalive: KeepaliveConfig,
}

impl RedisConfig {
    /// 验证配置有效性
    pub fn validate(&self) -> Result<()> {
        if self.addresses.is_empty() {
            return Err(SyncError::Config("Redis 地址列表不能为空".into()));
        }
        
        if self.redis_type == RedisType::Cluster && self.addresses.len() < 2 {
            return Err(SyncError::Config("Cluster 模式需要至少 2 个地址".into()));
        }
        
        Ok(())
    }
}

/// 回放配置
#[derive(Debug, Clone, Deserialize)]
pub struct ReplayConfig {
    /// 是否从断点续传
    #[serde(default)]
    pub resume_from_break_point: bool,
    /// Key 存在时的处理策略
    #[serde(default)]
    pub key_exists: KeyExistsPolicy,
    /// RDB 并行回放数
    #[serde(default = "default_rdb_parallel")]
    pub rdb_parallel: usize,
    /// 是否使用 Pipeline 模式
    #[serde(default)]
    pub pipeline: bool,
    /// 是否使用事务模式（MULTI/EXEC）
    #[serde(default)]
    pub transaction: bool,
    /// 批量大小（字节）
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// 批量计数
    #[serde(default = "default_batch_count")]
    pub batch_count: usize,
}

fn default_rdb_parallel() -> usize { DEFAULT_RDB_PARALLEL }
fn default_batch_size() -> usize { DEFAULT_BATCH_SIZE }
fn default_batch_count() -> usize { DEFAULT_BATCH_COUNT }

impl Default for ReplayConfig {
    fn default() -> Self {
        ReplayConfig {
            resume_from_break_point: false,
            key_exists: KeyExistsPolicy::None,
            rdb_parallel: DEFAULT_RDB_PARALLEL,
            pipeline: false,
            transaction: false,
            batch_size: DEFAULT_BATCH_SIZE,
            batch_count: DEFAULT_BATCH_COUNT,
        }
    }
}

/// 过滤配置
#[derive(Debug, Clone, Deserialize, Default)]
pub struct FilterConfig {
    /// DB 黑名单
    #[serde(default)]
    pub db_black_list: Vec<u32>,
    /// 命令黑名单
    #[serde(default)]
    pub cmd_black_list: Vec<String>,
    /// Key 前缀白名单
    #[serde(default)]
    pub key_prefix_white_list: Vec<String>,
    /// Key 前缀黑名单
    #[serde(default)]
    pub key_prefix_black_list: Vec<String>,
    /// Slot 白名单
    #[serde(default)]
    pub slot_white_list: Vec<SlotRange>,
    /// Slot 黑名单
    #[serde(default)]
    pub slot_black_list: Vec<SlotRange>,
}

/// 输入配置（源 Redis）
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputConfig {
    /// Redis 配置
    #[serde(flatten)]
    pub redis: RedisConfig,
    /// 回放配置
    #[serde(default)]
    pub replay: ReplayConfig,
    /// 过滤配置
    #[serde(default)]
    pub filter: FilterConfig,
}

impl InputConfig {
    /// 验证配置有效性
    pub fn validate(&self) -> Result<()> {
        self.redis.validate()?;
        if self.replay.rdb_parallel < 1 {
            return Err(SyncError::Config("rdb_parallel 必须大于 0".into()));
        }
        Ok(())
    }
}

/// 输出配置（目标 Redis）
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputConfig {
    /// Redis 配置
    #[serde(flatten)]
    pub redis: RedisConfig,
}

impl OutputConfig {
    /// 验证配置有效性
    pub fn validate(&self) -> Result<()> {
        self.redis.validate()?;
        Ok(())
    }
}

/// 本地缓存配置
#[derive(Debug, Clone, Deserialize)]
pub struct LocalCacheConfig {
    /// 存储目录
    #[serde(default = "default_store_dir")]
    pub dir: String,
    /// 最大存储大小（字节）
    #[serde(default = "default_max_storage_size")]
    pub max_size: usize,
    /// AOF 日志大小限制（字节）
    #[serde(default = "default_log_size")]
    pub log_size: usize,
    /// AOF 文件头部大小（字节）
    #[serde(default = "default_header_size")]
    pub header_size: usize,
}

fn default_store_dir() -> String { DEFAULT_STORE_DIR.to_string() }
fn default_max_storage_size() -> usize { DEFAULT_MAX_STORAGE_SIZE }
fn default_log_size() -> usize { DEFAULT_LOG_SIZE }
fn default_header_size() -> usize { DEFAULT_HEADER_SIZE }

impl Default for LocalCacheConfig {
    fn default() -> Self {
        LocalCacheConfig {
            dir: DEFAULT_STORE_DIR.to_string(),
            max_size: DEFAULT_MAX_STORAGE_SIZE,
            log_size: DEFAULT_LOG_SIZE,
            header_size: DEFAULT_HEADER_SIZE,
        }
    }
}

/// 日志配置
#[derive(Debug, Clone, Deserialize)]
pub struct LogConfig {
    /// 日志级别（trace/debug/info/warn/error）
    #[serde(default = "default_log_level")]
    pub level: String,
    /// 日志输出目录
    #[serde(default)]
    pub dir: Option<String>,
    /// 日志最大保留天数
    #[serde(default = "default_log_max_age")]
    pub max_age: usize,
    /// 日志文件最大数量
    #[serde(default = "default_log_max_files")]
    pub max_files: usize,
    /// 单日志文件最大大小（字节）
    #[serde(default = "default_log_max_size")]
    pub max_size: usize,
    /// 是否输出到 stdout
    #[serde(default = "default_true")]
    pub stdout: bool,
}

fn default_log_level() -> String { DEFAULT_LOG_LEVEL.to_string() }
fn default_log_max_age() -> usize { DEFAULT_LOG_MAX_AGE }
fn default_log_max_files() -> usize { DEFAULT_LOG_MAX_FILES }
fn default_log_max_size() -> usize { DEFAULT_LOG_MAX_SIZE }
fn default_true() -> bool { true }

impl Default for LogConfig {
    fn default() -> Self {
        LogConfig {
            level: DEFAULT_LOG_LEVEL.to_string(),
            dir: None,
            max_age: DEFAULT_LOG_MAX_AGE,
            max_files: DEFAULT_LOG_MAX_FILES,
            max_size: DEFAULT_LOG_MAX_SIZE,
            stdout: true,
        }
    }
}

impl LogConfig {
    /// 验证日志级别有效性
    pub fn validate(&self) -> Result<()> {
        let valid_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_levels.contains(&self.level.as_str()) {
            return Err(SyncError::Config(format!(
                "无效的日志级别: {}, 有效值为: {:?}",
                self.level, valid_levels
            )));
        }
        Ok(())
    }
}

/// License 配置（阶段一暂不实现）
#[derive(Debug, Clone, Deserialize, Default)]
pub struct LicenseConfig {
    pub key: Option<String>,
}

/// 集群配置（阶段四实现）
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ClusterConfig {
    /// 是否启用集群模式
    pub enabled: bool,
    /// 集群节点 ID
    pub node_id: Option<String>,
    /// 选举后端（redis/etcd）
    pub election_backend: Option<String>,
}

/// 服务器配置
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// HTTP 服务端口
    #[serde(default = "default_http_port")]
    pub http_port: u16,
    /// gRPC 服务端口
    #[serde(default = "default_grpc_port")]
    pub grpc_port: u16,
}

fn default_http_port() -> u16 { DEFAULT_HTTP_PORT }
fn default_grpc_port() -> u16 { DEFAULT_GRPC_PORT }

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            http_port: DEFAULT_HTTP_PORT,
            grpc_port: DEFAULT_GRPC_PORT,
        }
    }
}

/// 顶层配置结构体
#[derive(Debug, Clone, Deserialize)]
pub struct SyncConfig {
    /// License 配置
    #[serde(default)]
    pub license: LicenseConfig,
    /// 服务器配置
    #[serde(default)]
    pub server: ServerConfig,
    /// 集群配置
    #[serde(default)]
    pub cluster: ClusterConfig,
    /// 输入配置（源 Redis）
    pub input: InputConfig,
    /// 本地缓存配置
    #[serde(default)]
    pub local_cache: LocalCacheConfig,
    /// 输出配置（目标 Redis）
    pub output: OutputConfig,
    /// 日志配置
    #[serde(default)]
    pub log: LogConfig,
}

impl Default for SyncConfig {
    fn default() -> Self {
        // 创建一个最小配置（用于测试）
        SyncConfig {
            license: LicenseConfig::default(),
            server: ServerConfig::default(),
            cluster: ClusterConfig::default(),
            input: InputConfig {
                redis: RedisConfig {
                    addresses: vec!["127.0.0.1:6379".to_string()],
                    password: None,
                    auth_type: AuthType::None,
                    tls: TlsConfig::default(),
                    redis_type: RedisType::Standalone,
                    slots: None,
                    cluster_shards: None,
                    keepalive: KeepaliveConfig::default(),
                },
                replay: ReplayConfig::default(),
                filter: FilterConfig::default(),
            },
            local_cache: LocalCacheConfig::default(),
            output: OutputConfig {
                redis: RedisConfig {
                    addresses: vec!["127.0.0.1:6380".to_string()],
                    password: None,
                    auth_type: AuthType::None,
                    tls: TlsConfig::default(),
                    redis_type: RedisType::Standalone,
                    slots: None,
                    cluster_shards: None,
                    keepalive: KeepaliveConfig::default(),
                },
            },
            log: LogConfig::default(),
        }
    }
}

impl SyncConfig {
    /// 从配置文件加载配置
    /// 
    /// 支持 JSON 和 JSONC 格式：
    /// - `.json`: JSON 格式
    /// - `.jsonc`: JSONC 格式（简化处理，不支持注释）
    /// 
    /// 相对路径处理：
    /// - 配置文件中的相对路径（如 `local_cache.dir` 和 `log.dir`）会自动转换为绝对路径
    /// - 基准目录为可执行文件所在目录
    /// - 例如：`"dir": "cache"` → `/path/to/exe_dir/cache`
    /// - 绝对路径保持不变
    /// 
    /// # 参数
    /// - path: 配置文件路径
    /// 
    /// # 返回
    /// 加载后的配置结构体或错误
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| SyncError::Config(format!("读取配置文件失败: {}: {}", path.display(), e)))?;
        
        // 根据文件扩展名判断格式
        let extension = path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());
        
        let mut config: Self = match extension.as_deref() {
            Some("json") | Some("jsonc") => {
                // JSONC 格式需要去除注释（简化处理：直接使用 JSON 解析）
                // 注：JSONC 格式中的注释会导致解析失败，建议使用纯 JSON 格式
                serde_json::from_str(&content)
                    .map_err(|e| SyncError::Config(format!("解析 JSON 配置文件失败: {}", e)))?
            }
            _ => {
                // 默认尝试 JSON 格式
                serde_json::from_str(&content)
                    .map_err(|e| SyncError::Config(format!("解析配置文件失败（默认 JSON 格式）: {}", e)))?
            }
        };
        
        // 规范化路径（将相对路径转换为绝对路径）
        config.normalize_paths()?;
        
        config.validate()?;
        
        Ok(config)
    }

    /// 从 JSON 文件加载配置
    pub fn from_json_file(path: &Path) -> Result<Self> {
        Self::from_file(path)
    }

    /// 规范化路径配置
    /// 
    /// 将相对路径转换为基于可执行文件目录的绝对路径。
    /// - local_cache.dir: 缓存目录
    /// - log.dir: 日志目录
    fn normalize_paths(&mut self) -> Result<()> {
        // 获取可执行文件所在目录作为基准目录
        let base_dir = std::env::current_exe()
            .map_err(|e| SyncError::Config(format!("获取可执行文件路径失败: {}", e)))?
            .parent()
            .ok_or_else(|| SyncError::Config("无法获取可执行文件父目录".into()))?
            .to_path_buf();
        
        // 规范化 local_cache.dir
        if !self.local_cache.dir.is_empty() {
            let path = Path::new(&self.local_cache.dir);
            if !path.is_absolute() {
                self.local_cache.dir = base_dir.join(path)
                    .to_str()
                    .ok_or_else(|| SyncError::Config("路径转换失败".into()))?
                    .to_string();
            }
        }
        
        // 规范化 log.dir
        if let Some(dir) = &self.log.dir {
            if !dir.is_empty() {
                let path = Path::new(dir);
                if !path.is_absolute() {
                    self.log.dir = Some(
                        base_dir.join(path)
                            .to_str()
                            .ok_or_else(|| SyncError::Config("路径转换失败".into()))?
                            .to_string()
                    );
                }
            }
        }
        
        Ok(())
    }

    /// 验证所有配置有效性
    pub fn validate(&self) -> Result<()> {
        self.input.validate()?;
        self.output.validate()?;
        self.log.validate()?;
        
        // 验证本地缓存配置
        if self.local_cache.max_size < self.local_cache.log_size {
            return Err(SyncError::Config("max_size 必须大于 log_size".into()));
        }
        
        Ok(())
    }
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// 测试 RedisType 默认值
    #[test]
    fn test_redis_type_default() {
        let redis_type = RedisType::default();
        assert_eq!(redis_type, RedisType::Standalone);
    }

    /// 测试 SlotRange 包含检查
    #[test]
    fn test_slot_range_contains() {
        let range = SlotRange::new(0, 100);
        assert!(range.contains(0));
        assert!(range.contains(50));
        assert!(range.contains(100));
        assert!(!range.contains(101));
    }

    /// 测试 ReplayConfig 默认值
    #[test]
    fn test_replay_config_default() {
        let config = ReplayConfig::default();
        assert_eq!(config.rdb_parallel, DEFAULT_RDB_PARALLEL);
        assert_eq!(config.batch_size, DEFAULT_BATCH_SIZE);
        assert_eq!(config.batch_count, DEFAULT_BATCH_COUNT);
        assert!(!config.pipeline);
        assert!(!config.transaction);
    }

    /// 测试 LogConfig 验证有效级别
    #[test]
    fn test_log_config_valid_level() {
        let config = LogConfig {
            level: "info".to_string(),
            dir: None,
            max_age: 7,
            max_files: 10,
            max_size: 100,
            stdout: true,
        };
        assert!(config.validate().is_ok());
    }

    /// 测试 LogConfig 验证无效级别
    #[test]
    fn test_log_config_invalid_level() {
        let config = LogConfig {
            level: "invalid".to_string(),
            dir: None,
            max_age: 7,
            max_files: 10,
            max_size: 100,
            stdout: true,
        };
        assert!(config.validate().is_err());
    }

    /// 测试从 JSON 文件加载完整配置
    #[test]
    fn test_load_config_from_json() {
        let json_content = r#"
{
  "input": {
    "addresses": ["127.0.0.1:6379"],
    "password": "test123",
    "redis_type": "standalone"
  },
  "output": {
    "addresses": ["127.0.0.1:6380"]
  }
}
"#;
        
        let mut temp_file = NamedTempFile::with_suffix(".json").unwrap();
        temp_file.write_all(json_content.as_bytes()).unwrap();
        
        let config = SyncConfig::from_file(temp_file.path()).unwrap();
        
        assert_eq!(config.input.redis.addresses.len(), 1);
        assert_eq!(config.input.redis.addresses[0], "127.0.0.1:6379");
        assert_eq!(config.input.redis.password, Some("test123".to_string()));
        assert_eq!(config.input.redis.redis_type, RedisType::Standalone);
        assert_eq!(config.output.redis.addresses.len(), 1);
    }

    /// 测试从 JSON 文件加载最小配置
    #[test]
    fn test_load_minimal_json_config() {
        let json_content = r#"
{
  "input": {
    "addresses": ["127.0.0.1:6379"]
  },
  "output": {
    "addresses": ["127.0.0.1:6380"]
  }
}
"#;
        
        let mut temp_file = NamedTempFile::with_suffix(".json").unwrap();
        temp_file.write_all(json_content.as_bytes()).unwrap();
        
        let config = SyncConfig::from_file(temp_file.path()).unwrap();
        
        // 验证默认值
        assert_eq!(config.input.redis.redis_type, RedisType::Standalone);
        assert_eq!(config.input.replay.rdb_parallel, DEFAULT_RDB_PARALLEL);
        assert_eq!(config.log.level, DEFAULT_LOG_LEVEL);
        assert_eq!(config.server.http_port, DEFAULT_HTTP_PORT);
    }

    /// 测试路径规范化 - 相对路径转换为绝对路径
    #[test]
    fn test_normalize_relative_paths() {
        let json_content = r#"
{
  "input": {
    "addresses": ["127.0.0.1:6379"]
  },
  "output": {
    "addresses": ["127.0.0.1:6380"]
  },
  "local_cache": {
    "dir": "cache",
    "max_size": 1073741824,
    "log_size": 104857600,
    "header_size": 16
  },
  "log": {
    "level": "info",
    "dir": "logs",
    "max_age": 7,
    "max_files": 10,
    "max_size": 104857600,
    "stdout": true
  }
}
"#;
        
        let mut temp_file = NamedTempFile::with_suffix(".json").unwrap();
        temp_file.write_all(json_content.as_bytes()).unwrap();
        
        let config = SyncConfig::from_file(temp_file.path()).unwrap();
        
        // 验证路径已被转换为绝对路径
        assert!(Path::new(&config.local_cache.dir).is_absolute());
        assert!(config.local_cache.dir.ends_with("cache"));
        
        if let Some(log_dir) = &config.log.dir {
            assert!(Path::new(log_dir).is_absolute());
            assert!(log_dir.ends_with("logs"));
        }
    }

    /// 测试路径规范化 - 绝对路径保持不变
    #[test]
    fn test_normalize_absolute_paths() {
        let json_content = r#"
{
  "input": {
    "addresses": ["127.0.0.1:6379"]
  },
  "output": {
    "addresses": ["127.0.0.1:6380"]
  },
  "local_cache": {
    "dir": "/tmp/redis-ha-tool",
    "max_size": 1073741824,
    "log_size": 104857600,
    "header_size": 16
  },
  "log": {
    "level": "info",
    "dir": "/var/log/redis-ha-tool",
    "max_age": 7,
    "max_files": 10,
    "max_size": 104857600,
    "stdout": true
  }
}
"#;
        
        let mut temp_file = NamedTempFile::with_suffix(".json").unwrap();
        temp_file.write_all(json_content.as_bytes()).unwrap();
        
        let config = SyncConfig::from_file(temp_file.path()).unwrap();
        
        // 验证绝对路径保持不变
        assert_eq!(config.local_cache.dir, "/tmp/redis-ha-tool");
        assert_eq!(config.log.dir, Some("/var/log/redis-ha-tool".to_string()));
    }

    /// 测试加载不存在的配置文件
    #[test]
    fn test_load_nonexistent_config() {
        let result = SyncConfig::from_file(Path::new("/nonexistent/path.json"));
        assert!(result.is_err());
    }

    /// 测试 Redis 配置验证 - 空地址列表
    #[test]
    fn test_redis_config_empty_addresses() {
        let config = RedisConfig {
            addresses: vec![], // 空地址列表
            password: None,
            auth_type: AuthType::None,
            tls: TlsConfig::default(),
            redis_type: RedisType::Standalone,
            slots: None,
            cluster_shards: None,
            keepalive: KeepaliveConfig::default(),
        };
        assert!(config.validate().is_err());
    }

    /// 测试 InputConfig 验证 - rdb_parallel 必须 >= 1
    #[test]
    fn test_input_config_invalid_rdb_parallel() {
        let input_config = InputConfig {
            redis: RedisConfig {
                addresses: vec!["127.0.0.1:6379".to_string()],
                password: None,
                auth_type: AuthType::None,
                tls: TlsConfig::default(),
                redis_type: RedisType::Standalone,
                slots: None,
                cluster_shards: None,
                keepalive: KeepaliveConfig::default(),
            },
            replay: ReplayConfig {
                rdb_parallel: 0, // 无效值
                ..Default::default()
            },
            filter: FilterConfig::default(),
        };
        assert!(input_config.validate().is_err());
    }
}