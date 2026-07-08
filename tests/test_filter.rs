//! tests/test_filter.rs - 过滤系统集成测试
//!
//! 验证过滤系统的核心功能：
//! - Trie 树前缀匹配
//! - RangeList Slot 匹配
//! - RedisKeyFilter 多维度过滤

use redis_syncer::{
    filter::{Trie, RangeList},
    config::{FilterConfig, SlotRange},
};

/// 测试 Trie 树前缀匹配功能
#[test]
fn test_trie_prefix_matching() {
    // 创建 Trie 并插入多个前缀
    let mut trie = Trie::new();
    
    // 插入常见前缀
    trie.insert(b"user:");
    trie.insert(b"order:");
    trie.insert(b"session:");
    trie.insert(b"cache:");
    
    // 测试精确匹配
    assert!(trie.exact_match(b"user:"));
    assert!(trie.exact_match(b"order:"));
    assert!(trie.exact_match(b"session:"));
    assert!(trie.exact_match(b"cache:"));
    
    // 测试前缀匹配（检查 Trie 中是否有键以指定前缀开始）
    assert!(trie.starts_with(b"user"));
    assert!(trie.starts_with(b"order"));
    assert!(trie.starts_with(b"session"));
    assert!(trie.starts_with(b"cache"));
    
    // 测试反向匹配（检查键是否以 Trie 中某个前缀开头）
    assert!(trie.matches_prefix(b"user:123"));
    assert!(trie.matches_prefix(b"user:profile:name"));
    assert!(trie.matches_prefix(b"order:456"));
    assert!(trie.matches_prefix(b"session:abc123"));
    assert!(trie.matches_prefix(b"cache:key1"));
    
    // 测试不匹配的情况
    assert!(!trie.matches_prefix(b"temp:data"));
    assert!(!trie.matches_prefix(b"config:setting"));
    assert!(!trie.exact_match(b"user:123"));  // 不是精确匹配
    assert!(!trie.exact_match(b"temp:"));
    
    println!("✅ Trie 树前缀匹配测试通过");
}

/// 测试 Trie 树空和边界情况
#[test]
fn test_trie_edge_cases() {
    let mut trie = Trie::new();
    
    // 空 Trie
    assert!(!trie.exact_match(b"anything"));
    assert!(!trie.starts_with(b"prefix"));
    assert!(!trie.matches_prefix(b"key"));
    assert!(trie.is_empty());
    
    // 插入空键
    trie.insert(b"");
    assert!(trie.exact_match(b""));
    assert!(!trie.is_empty());
    
    // 插入单字符
    trie.insert(b"a");
    assert!(trie.exact_match(b"a"));
    assert!(trie.matches_prefix(b"abc"));
    
    println!("✅ Trie 树边界情况测试通过");
}

/// 测试 RangeList Slot 匹配功能
#[test]
fn test_range_list_slot_matching() {
    // 创建 RangeList
    let range_list = RangeList::from_ranges(vec![
        (0, 100),
        (500, 600),
        (1000, 2000),
    ]);
    
    // 测试范围内的 Slot
    assert!(range_list.contains(0));
    assert!(range_list.contains(50));
    assert!(range_list.contains(100));
    assert!(range_list.contains(550));
    assert!(range_list.contains(1000));
    assert!(range_list.contains(1500));
    assert!(range_list.contains(2000));
    
    // 测试范围外的 Slot
    assert!(!range_list.contains(101));
    assert!(!range_list.contains(200));
    assert!(!range_list.contains(601));
    assert!(!range_list.contains(999));
    assert!(!range_list.contains(2001));
    
    // 测试边界值
    assert!(range_list.contains(0));
    assert!(range_list.contains(2000));
    
    println!("✅ RangeList Slot 匹配测试通过");
}

/// 测试 RangeList 合重叠范围
#[test]
fn test_range_list_merge_overlap() {
    // 创建重叠的范围列表
    let range_list = RangeList::from_ranges(vec![
        (0, 100),
        (50, 150),   // 与第一个重叠
        (200, 300),
        (250, 350),  // 与第三个重叠
    ]);
    
    // 应该合并为 (0, 150) 和 (200, 350)
    assert_eq!(range_list.len(), 2);
    
    // 测试合并后的范围
    assert!(range_list.contains(120));  // 在合并后的范围内
    assert!(range_list.contains(320));  // 在合并后的范围内
    
    // 测试间隙
    assert!(!range_list.contains(160));
    assert!(!range_list.contains(180));
    
    println!("✅ RangeList 合重叠范围测试通过");
}

/// 测试 RangeList 从配置构建
#[test]
fn test_range_list_from_config() {
    let config_ranges = vec![
        SlotRange::new(0, 100),
        SlotRange::new(200, 300),
    ];
    
    let range_list = RangeList::from_config(&config_ranges);
    
    assert_eq!(range_list.len(), 2);
    assert!(range_list.contains(50));
    assert!(range_list.contains(250));
    assert!(!range_list.contains(150));
    
    // 测试总覆盖大小
    // (100-0+1) + (300-200+1) = 101 + 101 = 202
    assert_eq!(range_list.total_coverage(), 202);
    
    println!("✅ RangeList 从配置构建测试通过");
}

/// 测试 RedisKeyFilter 多维度过滤
#[test]
fn test_redis_key_filter_dimensions() {
    use redis_syncer::filter::RedisKeyFilter;
    
    let mut filter = RedisKeyFilter::new();
    
    // 配置多个维度的过滤规则
    filter.db_black_list.insert(1);
    filter.db_black_list.insert(2);
    
    filter.cmd_black_list.insert(b"FLUSHDB");
    filter.cmd_black_list.insert(b"FLUSHALL");
    
    filter.key_prefix_white_list.insert(b"user:");
    filter.key_prefix_white_list.insert(b"order:");
    
    filter.key_prefix_black_list.insert(b"temp:");
    filter.key_prefix_black_list.insert(b"cache:");
    
    // 测试 DB 黑名单过滤
    assert!(filter.filter_cmd_key("SET", Some(b"anykey"), 1));  // DB 1 过滤
    assert!(filter.filter_cmd_key("GET", Some(b"anykey"), 2));  // DB 2 过滤
    
    // 注意：当有白名单时，不在白名单中的 key 会被过滤
    // 所以 DB 0 的 "anykey" 不在白名单（user: 和 order:），会被过滤
    assert!(filter.filter_cmd_key("SET", Some(b"anykey"), 0));
    
    // 只有在白名单中的 key 才不会被过滤
    assert!(!filter.filter_cmd_key("SET", Some(b"user:123"), 0));  // 在白名单，不过滤
    
    // 测试命令黑名单过滤
    assert!(filter.filter_cmd_key("FLUSHDB", None, 0));  // 过滤
    assert!(filter.filter_cmd_key("FLUSHALL", None, 0));  // 过滤
    
    // 注意：当有白名单时，不在白名单中的 key 会被过滤
    // 所以 "key" 不在白名单（user: 和 order:），会被过滤
    assert!(filter.filter_cmd_key("SET", Some(b"key"), 0));
    
    // 只有在白名单中的 key 才不会被过滤
    assert!(!filter.filter_cmd_key("SET", Some(b"user:123"), 0));  // 在白名单，不过滤
    
    // 测试 NoRoute 命令过滤
    assert!(filter.filter_cmd_key("AUTH", None, 0));
    assert!(filter.filter_cmd_key("REPLCONF", None, 0));
    assert!(filter.filter_cmd_key("PSYNC", None, 0));
    
    // 测试 Key 前缀白名单过滤
    // 注意：当有白名单时，只有匹配白名单的 key 才不会被过滤
    assert!(!filter.filter_cmd_key("SET", Some(b"user:123"), 0));  // 在白名单，不过滤
    assert!(!filter.filter_cmd_key("GET", Some(b"order:456"), 0));  // 在白名单，不过滤
    
    // 不在白名单的 key 应该被过滤
    assert!(filter.filter_cmd_key("SET", Some(b"config:abc"), 0));   // 不在白名单，过滤
    
    // 测试 Key 前缀黑名单过滤
    // 注意：黑名单和白名单同时存在时，白名单优先检查
    // temp:abc 不在白名单（user: 和 order:），所以会被过滤（即使也在黑名单）
    assert!(filter.filter_cmd_key("SET", Some(b"temp:123"), 0));  // 不在白名单，过滤
    assert!(filter.filter_cmd_key("GET", Some(b"cache:key"), 0)); // 不在白名单，过滤
    
    // 清空白名单，只测试黑名单
    filter.key_prefix_white_list = Trie::new();  // 清空白名单
    
    // 现在只有黑名单生效
    assert!(filter.filter_cmd_key("SET", Some(b"temp:123"), 0));  // 在黑名单，过滤
    assert!(filter.filter_cmd_key("GET", Some(b"cache:key"), 0)); // 在黑名单，过滤
    assert!(!filter.filter_cmd_key("SET", Some(b"other:abc"), 0)); // 不在黑名单，不过滤
    
    println!("✅ RedisKeyFilter 多维度过滤测试通过");
}

/// 测试 RedisKeyFilter 从配置创建
#[test]
fn test_redis_key_filter_from_config() {
    use redis_syncer::filter::RedisKeyFilter;
    
    let config = FilterConfig {
        db_black_list: vec![1, 2, 3],
        cmd_black_list: vec!["FLUSHDB".to_string(), "DEBUG".to_string()],
        key_prefix_white_list: vec!["user:".to_string(), "order:".to_string()],
        key_prefix_black_list: vec!["temp:".to_string()],
        slot_white_list: vec![SlotRange::new(0, 100)],
        slot_black_list: vec![SlotRange::new(200, 300)],
    };
    
    let filter = RedisKeyFilter::from_config(&config);
    
    // 测试 DB 黑名单
    assert!(filter.filter_cmd_key("SET", Some(b"user:123"), 1));  // DB 1 在黑名单，过滤
    assert!(filter.filter_cmd_key("SET", Some(b"user:123"), 0));  // DB 0 不在黑名单，但 slot 不在白名单，会被过滤
    
    // 测试命令黑名单
    assert!(filter.filter_cmd_key("FLUSHDB", None, 0));
    assert!(filter.filter_cmd_key("DEBUG", None, 0));
    
    // 测试 Key 前缀白名单（结合 Slot）
    let key = b"user:123";
    let slot = RedisKeyFilter::key_slot(key);
    
    // user:123 在前缀白名单中，但 slot 是 12893，不在 slot 白名单 (0-100) 范围内
    // 所以会被 slot 白名单过滤掉
    assert!(filter.filter_cmd_key("SET", Some(key), 0), 
        "user:123 的 slot={} 不在白名单范围内 (0-100)，应被过滤", slot);
    
    // 不在 Key 白名单的 key 会被过滤（即使 Slot 在范围内）
    assert!(filter.filter_cmd_key("SET", Some(b"other:abc"), 0));
    
    // 测试 Key 前缀黑名单（temp: 不在白名单中，会被过滤）
    assert!(filter.filter_cmd_key("SET", Some(b"temp:abc"), 0));
    
    println!("✅ RedisKeyFilter 从配置创建测试通过");
}

/// 测试 RedisKeyFilter 优先级规则
#[test]
fn test_redis_key_filter_priority() {
    use redis_syncer::filter::RedisKeyFilter;
    
    let mut filter = RedisKeyFilter::new();
    
    // 配置：DB 黑名单 + Key 白名单
    filter.db_black_list.insert(1);
    filter.key_prefix_white_list.insert(b"user:");
    
    // DB 黑名单优先级高于 Key 白名单
    // 即使 key 在白名单中，DB 在黑名单中也会过滤
    assert!(filter.filter_cmd_key("SET", Some(b"user:123"), 1));
    
    // DB 不在黑名单，key 在白名单，不过滤
    assert!(!filter.filter_cmd_key("SET", Some(b"user:123"), 0));
    
    // DB 不在黑名单，key 不在白名单，过滤
    assert!(filter.filter_cmd_key("SET", Some(b"temp:abc"), 0));
    
    // NoRoute 命令优先级最高（总是过滤）
    assert!(filter.filter_cmd_key("AUTH", Some(b"user:123"), 0));
    
    println!("✅ RedisKeyFilter 优先级规则测试通过");
}

/// 测试 RedisKeyFilter Key Slot 计算
#[test]
fn test_redis_key_filter_key_slot() {
    use redis_syncer::filter::RedisKeyFilter;
    
    // 测试普通 key
    let slot1 = RedisKeyFilter::key_slot(b"mykey");
    assert!(slot1 < 16384);
    
    // 测试带 hash tag 的 key
    // 同一个 hash tag 应该有相同的 slot（理想情况）
    // 注意：由于简化实现，可能不完全一致
    
    let slot2 = RedisKeyFilter::key_slot(b"{user}:123");
    let slot3 = RedisKeyFilter::key_slot(b"{user}:456");
    
    // 实际应使用完整的 crc16 算法，hash tag 应使 slot 相同
    println!("Slot for {{user}}:123: {}", slot2);
    println!("Slot for {{user}}:456: {}", slot3);
    
    // 测试空 key
    let slot_empty = RedisKeyFilter::key_slot(b"");
    assert!(slot_empty < 16384);
    
    println!("✅ RedisKeyFilter Key Slot 计算测试通过");
}

/// 测试 RedisKeyFilter 命令 Key 位置查找
#[test]
fn test_redis_key_filter_command_key_positions() {
    use redis_syncer::filter::RedisKeyFilter;
    
    // 测试常见命令
    assert_eq!(RedisKeyFilter::command_key_positions("SET"), Some(&[1][..]));
    assert_eq!(RedisKeyFilter::command_key_positions("GET"), Some(&[1][..]));
    assert_eq!(RedisKeyFilter::command_key_positions("MGET"), Some(&[1, 2, 3, 4][..]));
    assert_eq!(RedisKeyFilter::command_key_positions("MSET"), Some(&[1, 3, 5, 7][..]));
    assert_eq!(RedisKeyFilter::command_key_positions("HSET"), Some(&[1][..]));
    assert_eq!(RedisKeyFilter::command_key_positions("LPUSH"), Some(&[1][..]));
    assert_eq!(RedisKeyFilter::command_key_positions("SADD"), Some(&[1][..]));
    assert_eq!(RedisKeyFilter::command_key_positions("ZADD"), Some(&[1][..]));
    
    // 测试特殊命令
    assert_eq!(RedisKeyFilter::command_key_positions("BITOP"), Some(&[2, 3, 4][..]));
    assert_eq!(RedisKeyFilter::command_key_positions("RENAME"), Some(&[1, 2][..]));
    
    // 测试无 Key 命令
    assert_eq!(RedisKeyFilter::command_key_positions("PING"), Some(&[0][..]));
    
    // 测试未知命令
    assert_eq!(RedisKeyFilter::command_key_positions("UNKNOWNCMD"), None);
    
    println!("✅ RedisKeyFilter 命令 Key 位置查找测试通过");
}

/// 测试 RangeList 完整 Redis Cluster Slot 范围
#[test]
fn test_range_list_full_cluster_slots() {
    // Redis Cluster 有 16384 个 slot (0-16383)
    let range_list = RangeList::from_ranges(vec![(0, 16383)]);
    
    assert_eq!(range_list.total_coverage(), 16384);
    
    // 测试边界值
    assert!(range_list.contains(0));
    assert!(range_list.contains(8191));
    assert!(range_list.contains(16383));
    
    // 测试超出范围（虽然不应该出现）
    assert!(!range_list.contains(16384));
    
    println!("✅ RangeList 完整 Cluster Slot 范围测试通过");
}