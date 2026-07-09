//! checkpoint/manager.rs - Checkpoint 管理器实现
//!
//! 本文件实现 CheckpointManager，支持读写检查点信息到目标 Redis Hash 结构。
//! 使用 master_replid 作为字段前缀，替代原先的 run_id 模式。

use std::time::SystemTime;
use redis::aio::MultiplexedConnection;
use crate::error::{SyncError, Result};
use crate::config::{CHECKPOINT_KEY, CHECKPOINT_KEY_HASH_KEY};

/// Checkpoint 信息结构
#[derive(Debug, Clone)]
pub struct CheckpointInfo {
    /// Checkpoint 名称（通常是 runId）
    pub key: String,
    /// Redis 主节点的复制 ID（master_replid）
    pub master_replid: String,
    /// 复制偏移量
    pub offset: i64,
    /// 版本号（用于防止并发冲突）
    pub version: u64,
    /// 最后修改时间
    pub mtime: SystemTime,
}

impl CheckpointInfo {
    /// 创建新的 Checkpoint
    pub fn new(master_replid: String, offset: i64) -> Self {
        CheckpointInfo {
            key: format!("{}_checkpoint", master_replid),
            master_replid,
            offset,
            version: 0,
            mtime: SystemTime::now(),
        }
    }
    
    /// 从 Redis Hash 字段构造
    pub fn from_hash_fields(master_replid: String, fields: std::collections::HashMap<String, String>) -> Result<Self> {
        let offset_str = fields.get(&format!("{}_offset", master_replid))
            .ok_or_else(|| SyncError::Corrupted(format!("缺少 {}_offset 字段", master_replid)))?;
        
        let offset: i64 = offset_str.parse()
            .map_err(|_| SyncError::Corrupted(format!("offset 解析失败: {}", offset_str)))?;
        
        let version_str = fields.get(&format!("{}_version", master_replid))
            .ok_or_else(|| SyncError::Corrupted(format!("缺少 {}_version 字段", master_replid)))?;
        
        let version: u64 = version_str.parse()
            .map_err(|_| SyncError::Corrupted(format!("version 解析失败: {}", version_str)))?;
        
        let mtime_str = fields.get(&format!("{}_mtime", master_replid))
            .ok_or_else(|| SyncError::Corrupted(format!("缺少 {}_mtime 字段", master_replid)))?;
        
        let mtime_unix: u64 = mtime_str.parse()
            .map_err(|_| SyncError::Corrupted(format!("mtime 解析失败: {}", mtime_str)))?;
        
        let mtime = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(mtime_unix);
        
        Ok(CheckpointInfo {
            key: format!("{}_checkpoint", master_replid),
            master_replid,
            offset,
            version,
            mtime,
        })
    }
}

/// PSYNC 信息字段后缀常量
/// 用于在 checkpoint hash 中标识不同类型的字段
const PSYNC_FIELD_SUFFIX_OFFSET: &str = "_offset";
const PSYNC_FIELD_SUFFIX_RUNID: &str = "_runid";
const PSYNC_FIELD_SUFFIX_VERSION: &str = "_version";
const PSYNC_FIELD_SUFFIX_MTIME: &str = "_mtime";

/// Checkpoint 管理器
///
/// 负责读写 Checkpoint 到目标 Redis。
pub struct CheckpointManager {
    /// Redis 连接
    conn: MultiplexedConnection,
}

impl CheckpointManager {
    /// 创建 Checkpoint 管理器
    ///
    /// # 参数
    /// - conn: Redis 异步连接
    pub fn new(conn: MultiplexedConnection) -> Self {
        CheckpointManager { conn }
    }
    
    /// 写入 Checkpoint 到 Redis
    ///
    /// 将 Checkpoint 信息写入目标 Redis 的 Hash 结构。
    ///
    /// # 参数
    /// - checkpoint: Checkpoint 信息
    ///
    /// # 返回
    /// 成功或错误
    pub async fn save_checkpoint(&self, checkpoint: &CheckpointInfo) -> Result<()> {
        let mut conn = self.conn.clone();
        
        let mtime_unix = checkpoint.mtime
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_secs();
        
        // 构建 Hash 字段（使用 master_replid 作为前缀）
        let fields = [
            (format!("{}_offset", checkpoint.master_replid), checkpoint.offset.to_string()),
            (format!("{}_runid", checkpoint.master_replid), checkpoint.master_replid.clone()),
            (format!("{}_version", checkpoint.master_replid), checkpoint.version.to_string()),
            (format!("{}_mtime", checkpoint.master_replid), mtime_unix.to_string()),
        ];
        
        // 使用 HSET 写入 Hash
        let key = CHECKPOINT_KEY.to_string();
        
        for (field, value) in fields.iter() {
            redis::cmd("HSET")
                .arg(&key)
                .arg(field)
                .arg(value)
                .query_async::<_, ()>(&mut conn)
                .await?;
        }
        
        // 更新 master_replid → checkpoint 映射
        redis::cmd("HSET")
            .arg(CHECKPOINT_KEY_HASH_KEY)
            .arg(&checkpoint.master_replid)
            .arg(&checkpoint.key)
            .query_async::<_, ()>(&mut conn)
            .await?;
        
        tracing::info!(
            "写入 Checkpoint: master_replid={}, offset={}, version={}",
            checkpoint.master_replid,
            checkpoint.offset,
            checkpoint.version
        );
        
        Ok(())
    }
    
    /// 从 Redis 读取 Checkpoint
    ///
    /// 从目标 Redis Hash 结构读取指定 master_replid 的 Checkpoint。
    ///
    /// # 参数
    /// - master_replid: Redis 主节点的复制 ID
    ///
    /// # 返回
    /// CheckpointInfo 或 None（不存在）
    pub async fn get_checkpoint(&self, master_replid: &str) -> Result<Option<CheckpointInfo>> {
        let mut conn = self.conn.clone();
        
        // 检查 master_replid 是否存在
        let exists: bool = redis::cmd("HEXISTS")
            .arg(CHECKPOINT_KEY_HASH_KEY)
            .arg(master_replid)
            .query_async(&mut conn)
            .await?;
        
        if !exists {
            return Ok(None);
        }
        
        // 读取所有字段
        let fields: std::collections::HashMap<String, String> = redis::cmd("HGETALL")
            .arg(CHECKPOINT_KEY)
            .query_async(&mut conn)
            .await?;
        
        if fields.is_empty() {
            return Ok(None);
        }
        
        // 构造 CheckpointInfo
        let checkpoint = CheckpointInfo::from_hash_fields(master_replid.to_string(), fields)?;
        
        Ok(Some(checkpoint))
    }
    
    /// 更新 Checkpoint offset
    ///
    /// 仅更新 offset 和 version，不修改其他字段。
    ///
    /// # 参数
    /// - master_replid: Redis 主节点的复制 ID
    /// - new_offset: 新的偏移量
    ///
    /// # 返回
    /// 成功或错误
    pub async fn update_offset(&self, master_replid: &str, new_offset: i64) -> Result<()> {
        let mut conn = self.conn.clone();
        
        // 先读取当前版本
        let current_version: Option<String> = redis::cmd("HGET")
            .arg(CHECKPOINT_KEY)
            .arg(format!("{}_version", master_replid))
            .query_async(&mut conn)
            .await?;
        
        let version: u64 = current_version
            .and_then(|v| v.parse().ok())
            .unwrap_or(0) + 1;
        
        // 更新 offset 和 version
        redis::cmd("HSET")
            .arg(CHECKPOINT_KEY)
            .arg(format!("{}_offset", master_replid))
            .arg(new_offset)
            .query_async::<_, ()>(&mut conn)
            .await?;
        
        redis::cmd("HSET")
            .arg(CHECKPOINT_KEY)
            .arg(format!("{}_version", master_replid))
            .arg(version)
            .query_async::<_, ()>(&mut conn)
            .await?;
        
        // 更新 mtime
        let mtime_unix = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_secs();
        
        redis::cmd("HSET")
            .arg(CHECKPOINT_KEY)
            .arg(format!("{}_mtime", master_replid))
            .arg(mtime_unix)
            .query_async::<_, ()>(&mut conn)
            .await?;
        
        tracing::info!(
            "更新 Checkpoint offset: master_replid={}, offset={}, version={}",
            master_replid,
            new_offset,
            version
        );
        
        Ok(())
    }
    
    /// 删除 Checkpoint
    ///
    /// 删除指定 master_replid 的 Checkpoint 信息。
    ///
    /// # 参数
    /// - master_replid: Redis 主节点的复制 ID
    ///
    /// # 返回
    /// 成功或错误
    pub async fn delete_checkpoint(&self, master_replid: &str) -> Result<()> {
        let mut conn = self.conn.clone();
        
        // 删除 Hash 字段
        let fields = [
            format!("{}_offset", master_replid),
            format!("{}_runid", master_replid),
            format!("{}_version", master_replid),
            format!("{}_mtime", master_replid),
        ];
        
        redis::cmd("HDEL")
            .arg(CHECKPOINT_KEY)
            .arg(&fields[0])
            .arg(&fields[1])
            .arg(&fields[2])
            .arg(&fields[3])
            .query_async::<_, ()>(&mut conn)
            .await?;
        
        // 删除 master_replid 映射
        redis::cmd("HDEL")
            .arg(CHECKPOINT_KEY_HASH_KEY)
            .arg(master_replid)
            .query_async::<_, ()>(&mut conn)
            .await?;
        
        tracing::info!("删除 Checkpoint: master_replid={}", master_replid);
        
        Ok(())
    }
    
    /// 获取所有 Checkpoint
    ///
    /// 获取目标 Redis 中所有的 Checkpoint 信息。
    ///
    /// # 返回
    /// master_replid → CheckpointInfo 映射
    pub async fn get_all_checkpoints(&self) -> Result<std::collections::HashMap<String, CheckpointInfo>> {
        let mut conn = self.conn.clone();
        
        // 先获取所有 master_replid
        let replid_map: std::collections::HashMap<String, String> = redis::cmd("HGETALL")
            .arg(CHECKPOINT_KEY_HASH_KEY)
            .query_async(&mut conn)
            .await?;
        
        let mut result = std::collections::HashMap::new();
        
        for master_replid in replid_map.keys() {
            if let Some(checkpoint) = self.get_checkpoint(master_replid).await? {
                result.insert(master_replid.clone(), checkpoint);
            }
        }
        
        Ok(result)
    }

    /// 保存 PSYNC 信息（master_replid + offset）到 checkpoint hash
    ///
    /// 使用 master_replid 的实际值作为字段名存储偏移量，
    /// 不依赖任何固定字段名。读取时通过 HGETALL 扫描自动发现。
    ///
    /// # 参数
    /// - master_replid: Redis 主节点的 Replication ID
    /// - offset: 复制偏移量
    pub async fn save_psync_info(&self, master_replid: &str, offset: i64) -> Result<()> {
        let mut conn = self.conn.clone();

        // 直接用 master_replid 的实际值作为字段名存储偏移量
        // 字段名 = 实际的 replid 值（如 "5e2f1b3a2c4d6e8f0a1b2c3d4e5f6a7b8c9d0e1f"）
        // 字段值 = 偏移量
        redis::cmd("HSET")
            .arg(CHECKPOINT_KEY)
            .arg(master_replid)
            .arg(offset)
            .query_async::<_, ()>(&mut conn)
            .await?;

        // 同时存储 <master_replid>_offset 字段（与 save_checkpoint 保持一致）
        let offset_field = format!("{}{}", master_replid, PSYNC_FIELD_SUFFIX_OFFSET);
        redis::cmd("HSET")
            .arg(CHECKPOINT_KEY)
            .arg(&offset_field)
            .arg(offset)
            .query_async::<_, ()>(&mut conn)
            .await?;

        tracing::info!(
            "保存 PSYNC 信息: master_replid={}, offset={}",
            master_replid, offset
        );

        Ok(())
    }

    /// 读取 PSYNC 信息（master_replid + offset）
    ///
    /// 根据已知的 master_replid 查询 checkpoint：
    /// 1. HGET redis_ha_tool_checkpoint_hash <master_replid> → 检查 checkpoint 是否存在
    /// 2. HGET redis_ha_tool_checkpoint <master_replid>_offset → 获取 offset
    ///
    /// # 参数
    /// - master_replid: 从源 Redis INFO REPLICATION 获取的 master_replid
    ///
    /// # 返回
    /// `Some(offset)` 或 `None`（无 checkpoint 信息）
    pub async fn get_psync_info(&self, master_replid: &str) -> Result<Option<i64>> {
        let mut conn = self.conn.clone();

        tracing::info!(
            "请求 PSYNC checkpoint 信息: master_replid={}",
            master_replid,
        );

        // 1. 检查 checkpoint 是否存在
        let exists: bool = redis::cmd("HEXISTS")
            .arg(CHECKPOINT_KEY_HASH_KEY)
            .arg(master_replid)
            .query_async(&mut conn)
            .await?;

        if !exists {
            tracing::info!("通过 `hget {} {}` 未找到 master_replid={} 的 checkpoint，将执行全量同步", CHECKPOINT_KEY_HASH_KEY, master_replid, master_replid);
            return Ok(None);
        }

        // 2. 获取 offset
        let offset_field = format!("{}_offset", master_replid);
        let offset_str: Option<String> = redis::cmd("HGET")
            .arg(CHECKPOINT_KEY)
            .arg(&offset_field)
            .query_async(&mut conn)
            .await?;
        
        tracing::info!(
            "hget {} {}",
            CHECKPOINT_KEY,
            offset_field,
        );

        match offset_str {
            Some(offset_str) => {
                let offset: i64 = offset_str.parse()
                    .map_err(|_| SyncError::Corrupted(format!("offset 解析失败: {}", offset_str)))?;
                tracing::info!(
                    "读取 PSYNC checkpoint 成功: master_replid={}, offset={}",
                    master_replid, offset
                );
                Ok(Some(offset))
            }
            None => {
                tracing::info!("checkpoint 存在但缺少 offset 字段，将执行全量同步");
                Ok(None)
            }
        }
    }
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;
    
    /// 测试 CheckpointInfo 创建
    #[test]
    fn test_checkpoint_info_new() {
        let replid = "5e2f1b3a2c4d6e8f0a1b2c3d4e5f6a7b8c9d0e1f".to_string();
        let checkpoint = CheckpointInfo::new(replid.clone(), 1000);
        
        assert_eq!(checkpoint.master_replid, replid);
        assert_eq!(checkpoint.offset, 1000);
        assert_eq!(checkpoint.key, format!("{}_checkpoint", replid));
        assert_eq!(checkpoint.version, 0);
    }
    
    /// 测试 CheckpointInfo 从 Hash 字段构造
    #[test]
    fn test_checkpoint_info_from_hash_fields() {
        let replid = "5e2f1b3a2c4d6e8f0a1b2c3d4e5f6a7b8c9d0e1f".to_string();
        let fields = std::collections::HashMap::from([
            (format!("{}_offset", replid), "1000".to_string()),
            (format!("{}_runid", replid), replid.clone()),
            (format!("{}_version", replid), "5".to_string()),
            (format!("{}_mtime", replid), "1234567890".to_string()),
        ]);
        
        let checkpoint = CheckpointInfo::from_hash_fields(replid.clone(), fields).unwrap();
        
        assert_eq!(checkpoint.master_replid, replid);
        assert_eq!(checkpoint.offset, 1000);
        assert_eq!(checkpoint.version, 5);
    }
    
    /// 测试 CheckpointInfo 缺少字段
    #[test]
    fn test_checkpoint_info_missing_field() {
        let replid = "5e2f1b3a2c4d6e8f0a1b2c3d4e5f6a7b8c9d0e1f".to_string();
        let fields = std::collections::HashMap::from([
            (format!("{}_offset", replid), "1000".to_string()),
            // 缺少 version 和 mtime
        ]);
        
        let result = CheckpointInfo::from_hash_fields(replid, fields);
        assert!(result.is_err());
    }
    
    /// 测试 CheckpointInfo 字段解析失败
    #[test]
    fn test_checkpoint_info_parse_error() {
        let replid = "5e2f1b3a2c4d6e8f0a1b2c3d4e5f6a7b8c9d0e1f".to_string();
        let fields = std::collections::HashMap::from([
            (format!("{}_offset", replid), "not_a_number".to_string()),
            (format!("{}_runid", replid), replid.clone()),
            (format!("{}_version", replid), "5".to_string()),
            (format!("{}_mtime", replid), "1234567890".to_string()),
        ]);
        
        let result = CheckpointInfo::from_hash_fields(replid, fields);
        assert!(result.is_err());
    }
    
    // 注意：CheckpointManager 的异步测试需要真实的 Redis 连接
    // 可以在集成测试中使用 mock 或真实的 Redis 实例
}