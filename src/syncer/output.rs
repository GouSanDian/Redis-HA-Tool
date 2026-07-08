//! syncer/output.rs - 数据输出实现
//!
//! 本文件实现 RedisOutput，向目标 Redis 写入数据。

use async_trait::async_trait;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use bytes::Bytes;
use crate::error::{SyncError, Result};
use crate::syncer::Output;
use crate::store::Reader;
use crate::config::OutputConfig;
use crate::protocol::resp::{encode_command_str, encode_command};
use crate::protocol::rdb::{RdbParser, BinEntry};

pub struct RedisOutput {
    config: Arc<OutputConfig>,
}

impl RedisOutput {
    pub fn new(config: Arc<OutputConfig>) -> Self {
        RedisOutput { config }
    }

    async fn connect_redis(&self) -> Result<TcpStream> {
        let address = &self.config.redis.addresses[0];
        tracing::debug!("连接目标 Redis: {}", address);
        
        let stream = TcpStream::connect(address).await?;
        tracing::debug!("TCP 连接成功: {}", address);
        
        Ok(stream)
    }

    async fn authenticate(&self, stream: &mut TcpStream) -> Result<()> {
        if let Some(password) = &self.config.redis.password {
            if !password.is_empty() {
                tracing::debug!("发送 AUTH 命令");
                let auth_cmd = encode_command_str("AUTH", &[password.as_str()]);
                stream.write_all(&auth_cmd).await?;
                
                let mut response = [0u8; 1024];
                let n = stream.read(&mut response).await?;
                let response_str = String::from_utf8_lossy(&response[..n]);
                
                if !response_str.contains("+OK") {
                    return Err(SyncError::Protocol(format!("AUTH 失败: {}", response_str)));
                }
                tracing::debug!("AUTH 成功");
            }
        }
        Ok(())
    }

    async fn send_rdb_data(&self, stream: &mut TcpStream, reader: &mut Box<dyn Reader>) -> Result<i64> {
        tracing::debug!("解析 RDB 数据并发送 RESTORE 命令到目标 Redis");

        // 获取 RDB 总大小
        let rdb_size = reader.size().unwrap_or(0);
        tracing::debug!("RDB Reader size: {} 字节", rdb_size);

        // 读取所有 RDB 数据到内存
        let mut rdb_data = Vec::new();
        let mut buf = vec![0u8; 8192];

        loop {
            let n = reader.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            rdb_data.extend_from_slice(&buf[..n]);
        }

        tracing::info!("RDB 数据读取完成: {} 字节 (期望 {} 字节)", rdb_data.len(), rdb_size);

        if rdb_data.is_empty() {
            return Ok(0);
        }

        // 收集所有 entries
        let mut entries: Vec<BinEntry> = Vec::new();
        let cursor = std::io::Cursor::new(rdb_data);
        let mut parser = RdbParser::new(cursor);

        // 解析 RDB
        let result = parser.parse(|entry: BinEntry| -> Result<()> {
            entries.push(entry);
            Ok(())
        }).await;

        if let Err(e) = result {
            tracing::error!("RDB 解析失败: {}", e);
            // 即使解析失败，也要返回原始字节数，以便 offset 能正确推进
            // 这样 AOF 数据可以从正确的位置开始
            return Ok(rdb_size);
        }

        // 发送 RESTORE 命令
        let mut sent_keys = 0i64;
        let mut current_db = 0u32;

        for entry in entries {
            // 如果数据库切换了，发送 SELECT 命令
            if entry.db != current_db {
                current_db = entry.db;
                let select_cmd = encode_command_str("SELECT", &[&current_db.to_string()]);
                stream.write_all(&select_cmd).await?;
                stream.flush().await?;

                // 读取响应
                let mut response = [0u8; 1024];
                let n = stream.read(&mut response).await?;
                let response_str = String::from_utf8_lossy(&response[..n]);
                if !response_str.contains("+OK") {
                    return Err(SyncError::Protocol(format!("SELECT 失败: {}", response_str)));
                }
            }

            // 发送 RESTORE 命令
            self.send_restore_command(stream, &entry).await?;
            sent_keys += 1;
        }

        tracing::info!("RDB 数据发送完成: {} 个 key, {} 字节", sent_keys, rdb_size);
        // 返回 RDB 原始字节数，而不是 key 数量
        Ok(rdb_size)
    }

    /// 发送 RESTORE 命令恢复单个 key
    ///
    /// 使用 DUMP 格式序列化数据，通过 RESTORE 命令恢复
    /// 支持所有数据类型（String, List, Set, Hash, ZSet 等）
    async fn send_restore_command(&self, stream: &mut TcpStream, entry: &BinEntry) -> Result<()> {
        let key = String::from_utf8_lossy(&entry.key);

        // 生成 DUMP 格式的序列化数据
        let dump_payload = match crate::protocol::rdb::generate_dump_payload(entry) {
            Some(payload) => payload,
            None => {
                tracing::warn!("无法生成 DUMP payload for key '{}': 缺少原始 RDB 数据", key);
                return Ok(());
            }
        };

        // 计算 TTL（毫秒）
        let ttl_ms = if let Some(expire) = entry.expire {
            let now = std::time::SystemTime::now();
            if expire > now {
                expire.duration_since(now).unwrap_or_default().as_millis() as i64
            } else {
                0 // 已过期，不设置 TTL
            }
        } else {
            0 // 无过期时间
        };

        // 构建 RESTORE 命令
        // RESTORE key ttl serialized-value [REPLACE]
        let ttl_str = ttl_ms.to_string();
        let restore_cmd = encode_command("RESTORE", &[
            entry.key.clone(),
            bytes::Bytes::from(ttl_str),
            dump_payload,
            bytes::Bytes::from("REPLACE"), // 使用 REPLACE 覆盖已存在的 key
        ]);

        stream.write_all(&restore_cmd).await?;
        stream.flush().await?;

        // 读取响应
        let mut response = [0u8; 1024];
        let n = stream.read(&mut response).await?;
        let response_str = String::from_utf8_lossy(&response[..n]);

        if !response_str.contains("+OK") {
            return Err(SyncError::Protocol(format!(
                "RESTORE 失败 for key '{}' (type: {:?}): {}",
                key, entry.rdb_type, response_str
            )));
        }

        tracing::debug!("RESTORE 成功: key='{}', type={:?}, ttl={}ms", key, entry.rdb_type, ttl_ms);

        Ok(())
    }

    async fn send_aof_data(&self, stream: &mut TcpStream, reader: &mut Box<dyn Reader>) -> Result<i64> {
        tracing::debug!("发送 AOF 命令流到目标 Redis");

        let mut buf = vec![0u8; 8192];
        let mut sent = 0i64;
        let mut total_read = 0i64;

        loop {
            // 使用 AsyncRead trait 的 read 方法
            use tokio::io::AsyncReadExt;
            let n = reader.read(&mut buf).await?;

            if n == 0 {
                tracing::debug!("AOF Reader 读取完成，总共读取 {} 字节", total_read);
                break;
            }

            total_read += n as i64;
            tracing::debug!("从 Reader 读取 {} 字节，累计 {} 字节", n, total_read);

            // 将 RESP 命令直接发送到目标 Redis
            stream.write_all(&buf[..n]).await?;
            stream.flush().await?;

            sent += n as i64;

            // 读取响应（对于 inline 命令，Redis 会返回响应）
            // 但 AOF 流通常包含多个命令，我们需要非阻塞地处理响应
            // 这里使用一个小的超时来读取响应，避免阻塞
            let mut response_buf = [0u8; 1024];
            match tokio::time::timeout(
                tokio::time::Duration::from_millis(10),
                stream.read(&mut response_buf)
            ).await {
                Ok(Ok(response_n)) if response_n > 0 => {
                    let response = String::from_utf8_lossy(&response_buf[..response_n]);
                    if response.starts_with("-ERR") {
                        tracing::error!("Redis 返回错误: {}", response.trim());
                    } else {
                        tracing::debug!("Redis 响应: {}", response.trim());
                    }
                }
                _ => {
                    // 超时或没有响应，继续发送
                }
            }

            if sent % (1024 * 1024) == 0 {
                tracing::debug!("AOF 发送进度: {} 字节", sent);
            }
        }

        tracing::info!("AOF 数据发送完成: {} 字节", sent);

        Ok(sent)
    }
}

#[async_trait]
impl Output for RedisOutput {
    async fn send(&self, mut reader: Box<dyn Reader>) -> Result<i64> {
        tracing::info!(
            "发送数据到目标 Redis: {:?}",
            self.config.redis.addresses
        );
        
        tracing::debug!("RedisOutput 配置信息:");
        tracing::debug!("  - Redis 类型: {:?}", self.config.redis.redis_type);
        tracing::debug!("  - 认证类型: {:?}", self.config.redis.auth_type);
        tracing::debug!("  - TLS 启用: {}", self.config.redis.tls.enabled);
        
        tracing::debug!("  - Reader 类型: {:?}", reader.reader_type());
        tracing::debug!("  - 数据偏移: {}", reader.offset());
        tracing::debug!("  - 数据大小: {:?}", reader.size());
        
        let mut stream = self.new_stream().await?;
        
        let sent = match reader.reader_type() {
            crate::store::ReaderType::Rdb => {
                self.send_rdb_data(&mut stream, &mut reader).await?
            }
            crate::store::ReaderType::Aof => {
                self.send_aof_data(&mut stream, &mut reader).await?
            }
        };
        
        tracing::info!("RedisOutput 数据发送完成: {} 字节", sent);
        
        Ok(sent)
    }
    
    async fn stop(&self) -> Result<()> {
        tracing::info!("停止 RedisOutput");
        tracing::debug!("关闭 RedisOutput 连接");
        Ok(())
    }
    
    async fn new_stream(&self) -> Result<TcpStream> {
        let mut stream = self.connect_redis().await?;
        self.authenticate(&mut stream).await?;
        Ok(stream)
    }
    
    async fn send_with_stream(&self, mut reader: Box<dyn Reader>, stream: &mut TcpStream) -> Result<i64> {
        let sent = match reader.reader_type() {
            crate::store::ReaderType::Rdb => {
                self.send_rdb_data(stream, &mut reader).await?
            }
            crate::store::ReaderType::Aof => {
                self.send_aof_data(stream, &mut reader).await?
            }
        };
        
        Ok(sent)
    }
}