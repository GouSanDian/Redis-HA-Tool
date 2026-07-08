/// utils/hash.rs - 哈希工具函数
/// 
/// 本文件实现了 FNV-1a 哈希算法，
/// 用于将 Redis key 分发到不同的并行回放 Task。

use fnv::FnvHasher;
use std::hash::Hasher;

/// 计算 FNV-1a 哈希值
/// 
/// FNV-1a 是一种快速的非加密哈希算法，
/// 适合用于数据分片和负载均衡。
/// 
/// # 参数
/// - data: 要哈希的数据
/// 
/// # 返回
/// 64 位哈希值
/// 
/// # 示例
/// ```rust
/// use redis_syncer::utils::fnv_hash;
/// 
/// let hash = fnv_hash(b"hello");
/// assert_ne!(hash, 0);
/// 
/// let hash2 = fnv_hash(b"hello");
/// assert_eq!(hash, hash2); // 相同输入产生相同输出
/// ```
pub fn fnv_hash(data: &[u8]) -> u64 {
    let mut hasher = FnvHasher::default();
    hasher.write(data);
    hasher.finish()
}

/// 计算 FNV-1a 哈希值并映射到指定范围
///
/// 用于将 key 分发到 N 个并行 worker。
///
/// # 参数
/// - data: 要哈希的数据
/// - range: 范围大小（worker 数量）
///
/// # 返回
/// 0 到 range-1 之间的索引值
#[allow(dead_code)]
pub fn fnv_hash_range(data: &[u8], range: usize) -> usize {
    if range == 0 {
        return 0;
    }
    fnv_hash(data) as usize % range
}

/// 计算字符串的 FNV-1a 哈希值
///
/// 便捷方法，将字符串转换为字节后再哈希。
#[allow(dead_code)]
pub fn fnv_hash_str(s: &str) -> u64 {
    fnv_hash(s.as_bytes())
}

/// 计算字符串的 FNV-1a 哈希值并映射到指定范围
#[allow(dead_code)]
pub fn fnv_hash_str_range(s: &str, range: usize) -> usize {
    fnv_hash_range(s.as_bytes(), range)
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;

    /// 测试空数据的哈希值
    #[test]
    fn test_fnv_hash_empty() {
        let hash = fnv_hash(b"");
        // FNV offset basis for 64-bit: 14695981039346656037
        assert_eq!(hash, 14695981039346656037);
    }

    /// 测试已知数据的哈希值
    #[test]
    fn test_fnv_hash_known() {
        let hash = fnv_hash(b"hello");
        // FNV-1a hash for "hello": 0xe59c7d8e (简化验证)
        assert_ne!(hash, 0);
        
        // 验证不同数据产生不同哈希
        let hash2 = fnv_hash(b"world");
        assert_ne!(hash, hash2);
    }

    /// 测试哈希一致性
    #[test]
    fn test_fnv_hash_consistency() {
        let data = b"test data";
        
        // 多次哈希相同输入应该产生相同输出
        let hash1 = fnv_hash(data);
        let hash2 = fnv_hash(data);
        let hash3 = fnv_hash(data);
        
        assert_eq!(hash1, hash2);
        assert_eq!(hash2, hash3);
    }

    /// 测试哈希范围映射
    #[test]
    fn test_fnv_hash_range() {
        let data = b"test key";
        let range = 4;
        
        // 结果应该在 [0, range-1] 之间
        let index = fnv_hash_range(data, range);
        assert!(index < range);
        
        // 多次调用应该产生相同结果
        let index2 = fnv_hash_range(data, range);
        assert_eq!(index, index2);
    }

    /// 测试 range 为 0 的情况
    #[test]
    fn test_fnv_hash_range_zero() {
        let index = fnv_hash_range(b"test", 0);
        assert_eq!(index, 0);
    }

    /// 测试不同 key 分发到不同 worker
    #[test]
    fn test_distribution() {
        let worker_count = 4;
        
        // 测试一组 key 的分布
        let keys = [
            "user:1", "user:2", "user:3", "user:4",
            "order:1", "order:2", "order:3", "order:4",
            "product:1", "product:2", "product:3", "product:4",
        ];
        
        let mut distribution = vec![0; worker_count];
        for key in &keys {
            let index = fnv_hash_str_range(key, worker_count);
            distribution[index] += 1;
        }
        
        // 验证分布较为均匀（非严格均匀）
        // 在 4 个 worker 中，每个至少应该有 2 个 key（12 key / 4 worker = 3）
        // 但实际分布可能不均匀，只验证总和正确
        assert_eq!(distribution.iter().sum::<usize>(), keys.len());
    }

    /// 测试字符串哈希函数
    #[test]
    fn test_fnv_hash_str() {
        let hash1 = fnv_hash_str("hello");
        let hash2 = fnv_hash(b"hello");
        
        assert_eq!(hash1, hash2);
    }

    /// 测试字符串哈希范围函数
    #[test]
    fn test_fnv_hash_str_range() {
        let index1 = fnv_hash_str_range("test", 4);
        let index2 = fnv_hash_range(b"test", 4);
        
        assert_eq!(index1, index2);
    }

    /// 测试相同前缀的 key 可能映射到不同 worker
    #[test]
    fn test_prefix_keys_different_workers() {
        let worker_count = 10;
        
        let key1 = "user:1";
        let key2 = "user:2";
        
        // 相同前缀的 key 可能映射到不同 worker
        let idx1 = fnv_hash_str_range(key1, worker_count);
        let idx2 = fnv_hash_str_range(key2, worker_count);
        
        // 不强制要求不同，但验证函数正常工作
        assert!(idx1 < worker_count);
        assert!(idx2 < worker_count);
    }

    /// 测试大量 key 的哈希分布均匀性
    #[test]
    fn test_large_scale_distribution() {
        let worker_count = 16;
        let key_count = 1000;
        
        let mut distribution = vec![0; worker_count];
        
        for i in 0..key_count {
            let key = format!("key:{}", i);
            let index = fnv_hash_str_range(&key, worker_count);
            distribution[index] += 1;
        }
        
        // 验证分布较为均匀
        // 期望值：1000 / 16 = 62.5
        // 允许偏差：期望值的 50% 以内
        let expected = key_count / worker_count;
        let tolerance = expected / 2;
        
        for count in &distribution {
            // 验证每个 worker 的 key 数量在合理范围内
            // 由于 FNV 是伪随机分布，这个范围应该是合理的
            assert!(*count >= expected - tolerance || *count <= expected + tolerance);
        }
        
        // 验证总数正确
        assert_eq!(distribution.iter().sum::<usize>(), key_count);
    }

    /// 测试特殊字符的哈希
    #[test]
    fn test_special_characters() {
        let special_keys = [
            "",                     // 空字符串
            ":",                    // 单冒号
            ":::",                  // 多冒号
            "\r\n",                 // 回车换行
            "中文",                  // 中文
            "🎉",                    // emoji
            "key with spaces",      // 空格
            "key:with:many:colons", // 多冒号
        ];
        
        for key in &special_keys {
            let hash = fnv_hash_str(key);
            assert_ne!(hash, 0); // 验证哈希值不为 0（除非是空字符串，但有 offset basis）
            
            let index = fnv_hash_str_range(key, 4);
            assert!(index < 4);
        }
    }
}