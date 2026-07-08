//! tests/test_filter_integration.rs - 过滤功能集成测试
//!
//! 验证：
//!   - DB 黑名单生效
//!   - Key 前缀过滤生效
//!   - 命令黑名单生效

use redis_syncer::{
    filter::RedisKeyFilter,
    config::FilterConfig,
};

/// 测试：DB 黑名单过滤
#[test]
fn test_db_blacklist_integration() {
    let config = FilterConfig {
        db_black_list: vec![1, 2, 3],
        ..Default::default()
    };
    
    let filter = RedisKeyFilter::from_config(&config);
    
    // DB 0 不过滤
    assert!(!filter.filter_cmd_key("SET", Some(b"user:123"), 0));
    
    // DB 1, 2, 3 过滤
    assert!(filter.filter_cmd_key("SET", Some(b"user:123"), 1));
    assert!(filter.filter_cmd_key("SET", Some(b"user:123"), 2));
    assert!(filter.filter_cmd_key("SET", Some(b"user:123"), 3));
    
    // DB 4 不过滤
    assert!(!filter.filter_cmd_key("SET", Some(b"user:123"), 4));
    
    tracing::info!("DB 黑名单集成测试通过");
}

/// 测试：命令黑名单过滤
#[test]
fn test_command_blacklist_integration() {
    let config = FilterConfig {
        cmd_black_list: vec!["FLUSHDB".to_string(), "FLUSHALL".to_string(), "DEBUG".to_string()],
        ..Default::default()
    };
    
    let filter = RedisKeyFilter::from_config(&config);
    
    // 命令黑名单生效
    assert!(filter.filter_cmd_key("FLUSHDB", None, 0));
    assert!(filter.filter_cmd_key("FLUSHALL", None, 0));
    assert!(filter.filter_cmd_key("DEBUG", None, 0));
    
    // 其他命令不过滤
    assert!(!filter.filter_cmd_key("SET", Some(b"key"), 0));
    assert!(!filter.filter_cmd_key("GET", Some(b"key"), 0));
    
    // NoRoute 命令总是过滤
    assert!(filter.filter_cmd_key("AUTH", None, 0));
    assert!(filter.filter_cmd_key("PSYNC", None, 0));
    
    tracing::info!("命令黑名单集成测试通过");
}

/// 测试：Key 前缀白名单过滤
#[test]
fn test_key_prefix_whitelist_integration() {
    let config = FilterConfig {
        key_prefix_white_list: vec!["user:".to_string(), "order:".to_string()],
        ..Default::default()
    };
    
    let filter = RedisKeyFilter::from_config(&config);
    
    // 白名单内的 key 不过滤
    assert!(!filter.filter_cmd_key("SET", Some(b"user:123"), 0));
    assert!(!filter.filter_cmd_key("SET", Some(b"user:profile:name"), 0));
    assert!(!filter.filter_cmd_key("SET", Some(b"order:456"), 0));
    
    // 白名单外的 key 过滤
    assert!(filter.filter_cmd_key("SET", Some(b"temp:abc"), 0));
    assert!(filter.filter_cmd_key("SET", Some(b"cache:key"), 0));
    assert!(filter.filter_cmd_key("SET", Some(b"config:setting"), 0));
    
    tracing::info!("Key 前缀白名单集成测试通过");
}

/// 测试：Key 前缀黑名单过滤
#[test]
fn test_key_prefix_blacklist_integration() {
    let config = FilterConfig {
        key_prefix_black_list: vec!["temp:".to_string(), "cache:".to_string()],
        ..Default::default()
    };
    
    let filter = RedisKeyFilter::from_config(&config);
    
    // 黑名单内的 key 过滤
    assert!(filter.filter_cmd_key("SET", Some(b"temp:123"), 0));
    assert!(filter.filter_cmd_key("SET", Some(b"temp:abc"), 0));
    assert!(filter.filter_cmd_key("SET", Some(b"cache:key"), 0));
    
    // 黑名单外的 key 不过滤
    assert!(!filter.filter_cmd_key("SET", Some(b"user:123"), 0));
    assert!(!filter.filter_cmd_key("SET", Some(b"order:456"), 0));
    
    tracing::info!("Key 前缀黑名单集成测试通过");
}

/// 测试：多维度组合过滤
#[test]
fn test_combined_filter_integration() {
    let config = FilterConfig {
        db_black_list: vec![1, 2],
        cmd_black_list: vec!["FLUSHDB".to_string()],
        key_prefix_white_list: vec!["user:".to_string()],
        key_prefix_black_list: vec!["temp:".to_string()],
        ..Default::default()
    };
    
    let filter = RedisKeyFilter::from_config(&config);
    
    // DB 黑名单优先级最高
    assert!(filter.filter_cmd_key("SET", Some(b"user:123"), 1));  // DB 1，过滤
    
    // Key 白名单生效
    assert!(!filter.filter_cmd_key("SET", Some(b"user:123"), 0));  // 在白名单，不过滤
    assert!(filter.filter_cmd_key("SET", Some(b"other:abc"), 0));  // 不在白名单，过滤
    
    // Key 黑名单生效（不在白名单时）
    assert!(filter.filter_cmd_key("SET", Some(b"temp:abc"), 0));  // 在黑名单，过滤
    
    // 命令黑名单生效
    assert!(filter.filter_cmd_key("FLUSHDB", None, 0));  // 命令黑名单，过滤
    
    tracing::info!("多维度组合过滤集成测试通过");
}

/// 测试：过滤规则验证
#[test]
fn test_filter_rules_validation() {
    // 测试各种命令的过滤
    let config = FilterConfig {
        db_black_list: vec![1],
        ..Default::default()
    };
    
    let filter = RedisKeyFilter::from_config(&config);
    
    // 写命令
    assert!(!filter.filter_cmd_key("SET", Some(b"key"), 0));
    assert!(!filter.filter_cmd_key("HSET", Some(b"key"), 0));
    assert!(!filter.filter_cmd_key("LPUSH", Some(b"key"), 0));
    
    // 读命令
    assert!(!filter.filter_cmd_key("GET", Some(b"key"), 0));
    assert!(!filter.filter_cmd_key("HGET", Some(b"key"), 0));
    
    // 删除命令
    assert!(!filter.filter_cmd_key("DEL", Some(b"key"), 0));
    
    // DB 1 的所有命令过滤
    assert!(filter.filter_cmd_key("SET", Some(b"key"), 1));
    assert!(filter.filter_cmd_key("GET", Some(b"key"), 1));
    assert!(filter.filter_cmd_key("DEL", Some(b"key"), 1));
    
    tracing::info!("过滤规则验证测试通过");
}

/// 文档说明：如何测试过滤功能
///
/// # 真实测试步骤
///
/// 1. 创建配置文件（启用过滤）：
///    ```json
///    {
///      "input": {
///        "addresses": ["127.0.0.1:6379"],
///        "filter": {
///          "db_black_list": [1, 2],
///          "cmd_black_list": ["FLUSHDB"],
///          "key_prefix_white_list": ["user:"]
///        }
///      },
///      "output": {
///        "addresses": ["127.0.0.1:6380"]
///      }
///    }
///    ```
///
/// 2. 在源 Redis 中填充数据：
///    ```bash
///    # DB 0
///    redis-cli -p 6379 SET user:123 value
///    redis-cli -p 6379 SET temp:abc value
///    
///    # DB 1
///    redis-cli -p 6379 SELECT 1
///    redis-cli -p 6379 SET key value
///    ```
///
/// 3. 运行同步器：
///    ```bash
///    cargo run -- --config config/config.json
///    ```
///
/// 4. 验证过滤效果：
///    ```bash
///    # DB 0 - user:123 应同步（在白名单）
///    redis-cli -p 6380 GET user:123
///    
///    # DB 0 - temp:abc 不应同步（不在白名单）
///    redis-cli -p 6380 GET temp:abc
///    
///    # DB 1 的数据不应同步（在黑名单）
///    redis-cli -p 6380 SELECT 1
///    redis-cli -p 6380 GET key
///    ```
#[test]
fn test_filter_documentation() {
    tracing::info!("请参考文档进行真实过滤功能测试");
}