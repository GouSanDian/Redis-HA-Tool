//! cluster/redis_election.rs - Redis 选举实现
//!
//! 本文件实现基于 Redis 的 Leader 选举机制。
//!
//! 实现原理：
//! - 使用 SETNX（SET if Not eXists）实现分布式锁
//! - 使用 TTL 实现自动过期（防止死锁）
//! - 定期续期保持 Leader 地位

use std::sync::Arc;
use std::time::Duration;
use async_trait::async_trait;
use redis::aio::MultiplexedConnection;
use tokio::sync::RwLock;
use tokio::time::{interval, sleep};
use crate::error::{SyncError, Result};
use crate::cluster::{Cluster, Election};
use crate::config::ELECTION_PREFIX_KEY;

/// Redis 集群实现
///
/// 基于 Redis 的集群管理功能。
pub struct RedisCluster {
    /// Redis 连接
    conn: MultiplexedConnection,
    
    /// 是否已关闭
    closed: RwLock<bool>,
}

impl RedisCluster {
    /// 创建 Redis 集群
    ///
    /// # 参数
    /// - conn: Redis 连接
    pub fn new(conn: MultiplexedConnection) -> Self {
        RedisCluster {
            conn,
            closed: RwLock::new(false),
        }
    }
}

#[async_trait]
impl Cluster for RedisCluster {
    async fn close(&self) -> Result<()> {
        *self.closed.write().await = true;
        tracing::info!("关闭 Redis 集群连接");
        Ok(())
    }
    
    async fn new_election(&self, key: &str) -> Result<Box<dyn Election>> {
        let election_key = format!("{}{}", ELECTION_PREFIX_KEY, key);
        
        Ok(Box::new(RedisElection::new(
            self.conn.clone(),
            election_key,
        )))
    }
    
    async fn register(&self, key: &str, value: &str) -> Result<()> {
        let mut conn = self.conn.clone();
        
        // 使用 SETNX 注册节点
        let result: bool = redis::cmd("SETNX")
            .arg(key)
            .arg(value)
            .query_async(&mut conn)
            .await?;
        
        if result {
            tracing::info!("注册节点成功: key={}, value={}", key, value);
        } else {
            tracing::warn!("节点已存在: key={}", key);
        }
        
        Ok(())
    }
    
    async fn discover(&self, key: &str) -> Result<Vec<String>> {
        let mut conn = self.conn.clone();
        
        // 使用 KEYS 发现节点（简化实现）
        // 实际应使用 SCAN 避免阻塞
        let pattern = format!("{}*", key);
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(&pattern)
            .query_async(&mut conn)
            .await?;
        
        tracing::info!("发现节点: pattern={}, count={}", pattern, keys.len());
        
        Ok(keys)
    }
}

/// Redis 选举实现
///
/// 使用 Redis SETNX + TTL 实现 Leader 选举。
pub struct RedisElection {
    /// Redis 连接
    conn: MultiplexedConnection,
    
    /// 选举键
    key: String,
    
    /// Leader 标识（节点 ID）
    leader_value: RwLock<Option<String>>,
    
    /// 是否为 Leader
    is_leader: RwLock<bool>,
    
    /// TTL（秒）
    ttl: u64,
}

impl RedisElection {
    /// 创建选举对象
    ///
    /// # 参数
    /// - conn: Redis 连接
    /// - key: 选举键
    pub fn new(conn: MultiplexedConnection, key: String) -> Self {
        RedisElection {
            conn,
            key,
            leader_value: RwLock::new(None),
            is_leader: RwLock::new(false),
            ttl: 10, // 默认 10 秒 TTL
        }
    }
    
    /// 尝试获取锁
    async fn try_acquire(&self, value: &str) -> Result<bool> {
        let mut conn = self.conn.clone();
        
        // 使用 SET NX EX 命令
        let result: bool = redis::cmd("SET")
            .arg(&self.key)
            .arg(value)
            .arg("NX")  // Not exists
            .arg("EX")  // Expire
            .arg(self.ttl)
            .query_async(&mut conn)
            .await?;
        
        Ok(result)
    }
    
    /// 续期后台任务
    fn start_renew_task(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut renew_interval = interval(Duration::from_secs(self.ttl / 2));
            
            loop {
                renew_interval.tick().await;
                
                if *self.is_leader.read().await {
                    if let Err(e) = self.renew().await {
                        tracing::error!("续期失败: {}", e);
                        *self.is_leader.write().await = false;
                        break;
                    }
                } else {
                    break;
                }
            }
        });
    }
}

#[async_trait]
impl Election for RedisElection {
    async fn renew(&self) -> Result<()> {
        let mut conn = self.conn.clone();
        
        let value = self.leader_value.read().await;
        let value = value.as_ref()
            .ok_or_else(|| SyncError::Break("未设置 leader_value".into()))?;
        
        // 使用 GET 检查值是否匹配
        let current: Option<String> = redis::cmd("GET")
            .arg(&self.key)
            .query_async(&mut conn)
            .await?;
        
        match current {
            Some(current_value) if current_value == *value => {
                // 值匹配，续期 TTL
                redis::cmd("EXPIRE")
                    .arg(&self.key)
                    .arg(self.ttl)
                    .query_async::<_, ()>(&mut conn)
                    .await?;
                
                tracing::debug!("续期成功: key={}", self.key);
                Ok(())
            }
            
            Some(_) => {
                // 值不匹配，失去 Leader
                *self.is_leader.write().await = false;
                Err(SyncError::Role("失去 Leader 地位".into()))
            }
            
            None => {
                // 键不存在，失去 Leader
                *self.is_leader.write().await = false;
                Err(SyncError::Role("选举键已过期".into()))
            }
        }
    }
    
    async fn leader(&self) -> Result<String> {
        let mut conn = self.conn.clone();
        
        let value: Option<String> = redis::cmd("GET")
            .arg(&self.key)
            .query_async(&mut conn)
            .await?;
        
        match value {
            Some(v) => Ok(v),
            None => Err(SyncError::Break("无 Leader".into())),
        }
    }
    
    async fn campaign(&self, value: &str) -> Result<()> {
        // 尝试获取锁
        let acquired = self.try_acquire(value).await?;
        
        if acquired {
            // 成功成为 Leader
            *self.leader_value.write().await = Some(value.to_string());
            *self.is_leader.write().await = true;
            
            tracing::info!("成为 Leader: key={}, value={}", self.key, value);
            
            // 启动续期任务
            let arc_self = Arc::new(
                RedisElection::new(self.conn.clone(), self.key.clone())
            );
            arc_self.start_renew_task();
            
            Ok(())
        } else {
            // 竞选失败
            tracing::info!("竞选失败: key={}", self.key);
            Err(SyncError::Role("竞选失败".into()))
        }
    }
    
    async fn resign(&self) -> Result<()> {
        let mut conn = self.conn.clone();
        
        let value = self.leader_value.read().await;
        let value = value.as_ref()
            .ok_or_else(|| SyncError::Break("未设置 leader_value".into()))?;
        
        // 检查是否为 Leader
        let current: Option<String> = redis::cmd("GET")
            .arg(&self.key)
            .query_async(&mut conn)
            .await?;
        
        if let Some(current_value) = current {
            if current_value == *value {
                // 删除选举键
                redis::cmd("DEL")
                    .arg(&self.key)
                    .query_async::<_, ()>(&mut conn)
                    .await?;
                
                *self.is_leader.write().await = false;
                *self.leader_value.write().await = None;
                
                tracing::info!("退选成功: key={}", self.key);
                Ok(())
            } else {
                Err(SyncError::Role("值不匹配，无法退选".into()))
            }
        } else {
            Err(SyncError::Role("选举键不存在".into()))
        }
    }
    
    async fn is_leader(&self) -> Result<bool> {
        Ok(*self.is_leader.read().await)
    }
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;
    
    /// 测试 RedisCluster 创建
    #[test]
    fn test_redis_cluster_create() {
        // 注意：真实测试需要 Redis 连接
        tracing::info!("RedisCluster 创建测试（需要真实 Redis）");
    }
    
    /// 测试选举键名生成
    #[test]
    fn test_election_key_generation() {
        let key = "sync_leader";
        let election_key = format!("{}{}", ELECTION_PREFIX_KEY, key);
        
        assert!(election_key.contains(ELECTION_PREFIX_KEY));
        
        tracing::info!("选举键生成测试通过: {}", election_key);
    }
    
    /// 测试 RedisElection 创建
    #[test]
    fn test_redis_election_create() {
        // 注意：真实测试需要 Redis 连接
        tracing::info!("RedisElection 创建测试（需要真实 Redis）");
    }
}