//! syncer/input.rs - 数据输入实现
//!
//! 本文件实现 RedisInput，从源 Redis 读取数据。
//! 使用 PSYNC 协议替代原始的 SYNC 协议，支持增量同步。

use async_trait::async_trait;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::Notify;
use crate::error::{SyncError, Result};
use crate::syncer::{Input, Channel};
use crate::config::InputConfig;
use crate::protocol::resp::encode_command_str;

pub struct RedisInput {
    config: Arc<InputConfig>,
    channel: Arc<dyn Channel>,
    run_id: String,
    /// PSYNC 响应中收到的偏移量（共享给 SyncerImpl 用于转发任务）
    psync_offset: Arc<AtomicI64>,
    /// PSYNC 响应已就绪通知器（共享给 SyncerImpl 用于转发任务）
    psync_ready: Arc<Notify>,
    /// 从 checkpoint 读取的 master_replid（用于发送 PSYNC 命令）
    initial_master_replid: String,
    /// 从 checkpoint 读取的偏移量（用于发送 PSYNC 命令）
    initial_master_offset: i64,
    /// PSYNC 响应中获取的 master_replid（共享给 SyncerImpl 用于保存到 checkpoint）
    psync_master_replid: Arc<Mutex<String>>,
}

impl RedisInput {
    pub fn new(
        config: Arc<InputConfig>,
        channel: Arc<dyn Channel>,
        run_id: String,
        psync_offset: Arc<AtomicI64>,
        psync_ready: Arc<Notify>,
        initial_master_replid: String,
        initial_master_offset: i64,
        psync_master_replid: Arc<Mutex<String>>,
    ) -> Self {
        RedisInput {
            config, channel, run_id,
            psync_offset, psync_ready,
            initial_master_replid, initial_master_offset,
            psync_master_replid,
        }
    }

    async fn connect_redis(&self) -> Result<TcpStream> {
        let address = &self.config.redis.addresses[0];
        tracing::debug!("连接 Redis: {}", address);

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

    /// 从源 Redis 获取 master_replid
    ///
    /// 通过 `INFO REPLICATION` 命令获取源 Redis 的 `master_replid`。
    /// 这是启动时确定当前数据集历史分支的第一步。
    pub async fn fetch_master_replid(&self) -> Result<String> {
        let mut stream = self.connect_redis().await?;
        self.authenticate(&mut stream).await?;

        // 发送 INFO REPLICATION
        let info_cmd = encode_command_str("INFO", &["REPLICATION"]);
        stream.write_all(&info_cmd).await?;
        stream.flush().await?;

        // 读取响应（bulk string 格式）
        let mut response = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = stream.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            response.extend_from_slice(&buf[..n]);
            // 简单判断：响应以 \r\n\r\n 结尾表示 INFO 输出结束
            if response.ends_with(b"\r\n\r\n") {
                break;
            }
        }

        let response_str = String::from_utf8_lossy(&response);

        // 解析 master_replid 行
        // 格式: master_replid:<40位十六进制字符串>
        for line in response_str.lines() {
            if let Some(replid) = line.strip_prefix("master_replid:") {
                let replid = replid.trim().to_string();
                if !replid.is_empty() && replid != "?" {
                    tracing::info!("从 INFO REPLICATION 获取 master_replid: {}", replid);
                    return Ok(replid);
                }
            }
        }

        Err(SyncError::Protocol(
            "INFO REPLICATION 响应中未找到有效的 master_replid".to_string(),
        ))
    }

    /// 设置初始 master_replid 和 offset（从 checkpoint 获取后调用）
    pub fn set_initial_psync(&mut self, master_replid: String, offset: i64) {
        self.initial_master_replid = master_replid;
        self.initial_master_offset = offset;
    }

    /// 发送 PSYNC 命令（替代旧的 SYNC 命令）
    ///
    /// PSYNC 格式: `PSYNC <runid> <offset>`
    /// - 从 checkpoint 获取到 master_replid + offset 时发送 `PSYNC <replid> <offset>`
    /// - 无 checkpoint 信息时发送 `PSYNC ? -1`（请求全量同步）
    async fn send_psync_command(&self, stream: &mut TcpStream) -> Result<()> {
        let (cmd_str, replid, offset) = if !self.initial_master_replid.is_empty() {
            // 有 checkpoint 信息：尝试增量同步
            let offset_str = self.initial_master_offset.to_string();
            let cmd = encode_command_str("PSYNC", &[&self.initial_master_replid, &offset_str]);
            (cmd, self.initial_master_replid.clone(), self.initial_master_offset)
        } else {
            // 无 checkpoint 信息：全量同步
            let cmd = encode_command_str("PSYNC", &["?", "-1"]);
            (cmd, "?".to_string(), -1)
        };

        let cmd_display = String::from_utf8_lossy(&cmd_str);
        tracing::info!(
            "发送 PSYNC 命令: {} （命令内容: {}字节）",
            cmd_display, cmd_str.len()
        );

        // 将发送的 replid 也写入共享状态，后续保存 checkpoint 时使用
        if replid != "?" {
            *self.psync_master_replid.lock().unwrap() = replid.clone();
            self.psync_offset.store(offset, Ordering::Relaxed);
        }

        stream.write_all(&cmd_str).await?;
        stream.flush().await?;
        tracing::debug!("PSYNC 命令已发送");
        Ok(())
    }

    /// 处理 PSYNC 响应
    ///
    /// PSYNC 有两种可能的响应：
    /// - `+FULLRESYNC <runid> <offset>\r\n` + RDB 数据 + AOF 流
    /// - `+CONTINUE\r\n` + AOF 流（增量同步，无需 RDB）
    async fn handle_psync_response(&self, buf_reader: &mut BufReader<TcpStream>) -> Result<()> {
        let mut first_line = String::new();
        buf_reader.read_line(&mut first_line).await?;
        let response = first_line.trim().to_string();
        tracing::info!("PSYNC 响应: {}", response);

        if response.starts_with("+FULLRESYNC") {
            // 例: +FULLRESYNC 5e2f1b3a2c4d6e8f0a1b2c3d4e5f6a7b8c9d0e1f 12345
            let parts: Vec<&str> = response.split_whitespace().collect();
            if parts.len() >= 3 {
                let master_replid = parts[1].to_string();
                if let Ok(offset) = parts[2].parse::<i64>() {
                    self.psync_offset.store(offset, Ordering::Relaxed);
                    *self.psync_master_replid.lock().unwrap() = master_replid.clone();
                    tracing::info!(
                        "FULLRESYNC: master_replid={}, master_offset={}",
                        master_replid, offset
                    );
                }
            }

            // 读取 RDB 数据（bulk string: $<size>\r\n<data>）
            // RDB 使用 master_offset 作为本地文件的全局 offset，
            // 这样转发任务用 master offset 查找时可以命中 RDB 文件。
            let master_offset = self.psync_offset.load(Ordering::Relaxed);
            let rdb_size = self.receive_rdb_data(buf_reader, master_offset).await?;

            // AOF 紧跟 RDB 之后，全局 offset = master_offset + rdb_size。
            // 这样转发任务处理完 RDB 后 offset 推进到这里，能正确命中 AOF 文件。
            let aof_start_offset = master_offset + rdb_size;
            tracing::info!(
                "AOF 起始全局 offset: {} (master_offset={} + rdb_size={})",
                aof_start_offset, master_offset, rdb_size
            );

            // RDB 已落盘、AOF writer 即将创建，此时再通知转发任务开始，
            // 避免转发任务在数据文件就绪前空转。
            self.psync_ready.notify_waiters();

            // 读取 AOF 流
            self.receive_aof_stream(buf_reader, aof_start_offset).await?;
        } else if response.starts_with("+CONTINUE") {
            // 增量同步: 无需 RDB，直接开始 AOF 命令流
            tracing::info!("收到 +CONTINUE，开始增量同步（跳过 RDB 阶段）");
            let offset = self.psync_offset.load(Ordering::Relaxed);
            tracing::info!("增量同步起始 offset: {}", offset);

            // 通知 Syncer 负责人转发任务可以开始了
            self.psync_ready.notify_waiters();

            // 不需要 RDB 同步，直接读取 AOF 流
            self.receive_aof_stream(buf_reader, offset).await?;
        } else {
            return Err(SyncError::Protocol(format!(
                "未知的 PSYNC 响应: {}",
                response
            )));
        }

        Ok(())
    }

    async fn receive_rdb_data(&self, buf_reader: &mut BufReader<TcpStream>, rdb_offset: i64) -> Result<i64> {
        tracing::debug!("读取 FULLRESYNC RDB 响应（全局 offset={}）", rdb_offset);

        let mut line = String::new();
        buf_reader.read_line(&mut line).await?;

        let line = line.trim();

        if !line.starts_with('$') {
            return Err(SyncError::Protocol(format!("无效的 RDB 响应头: {}", line)));
        }

        let size_str = line[1..].trim();
        let rdb_size: i64 = size_str
            .parse()
            .map_err(|e| SyncError::Protocol(format!("解析 RDB 大小失败: {}", e)))?;

        tracing::info!("RDB 大小: {} 字节", rdb_size);

        if rdb_size <= 0 {
            return Err(SyncError::Protocol("RDB 大小无效".into()));
        }

        // RDB 文件使用 master_offset 作为全局 offset，
        // 这样转发任务用 psync_offset（= master_offset）查找时可以命中。
        let mut rdb_writer = self.channel.get_rdb_writer(&self.run_id, rdb_offset, rdb_size).await?;

        tracing::debug!("开始接收 RDB 数据");

        let mut received = 0i64;
        let mut buf = vec![0u8; 8192];

        while received < rdb_size {
            let to_read = std::cmp::min(buf.len() as i64, rdb_size - received) as usize;
            let n = buf_reader.read(&mut buf[..to_read]).await?;

            if n == 0 {
                return Err(SyncError::Protocol("连接关闭，RDB 数据接收未完成".into()));
            }

            rdb_writer.write_all(&buf[..n]).await?;
            received += n as i64;

            if received % (1024 * 1024) == 0 {
                tracing::debug!("RDB 接收进度: {} / {} 字节", received, rdb_size);
            }
        }

        rdb_writer.flush().await?;
        tracing::info!("RDB 数据接收完成: {} 字节", rdb_size);

        Ok(rdb_size)
    }

    async fn receive_aof_stream(&self, buf_reader: &mut BufReader<TcpStream>, initial_offset: i64) -> Result<()> {
        tracing::debug!("开始接收 AOF 命令流（起始 offset: {}）", initial_offset);

        let mut aof_writer = self.channel.get_aof_writer(&self.run_id, initial_offset).await?;
        let mut buf = vec![0u8; 8192];
        let mut received = 0i64;

        loop {
            let n = buf_reader.read(&mut buf).await?;

            if n == 0 {
                tracing::info!("AOF 流结束");
                break;
            }

            aof_writer.write_all(&buf[..n]).await?;
            // 每次写入后立即 flush，确保数据落盘并通知转发任务
            aof_writer.flush().await?;
            received += n as i64;

            if received % (1024 * 1024) == 0 {
                tracing::debug!("AOF 接收进度: {} 字节", received);
            }
        }

        aof_writer.flush().await?;
        tracing::info!("AOF 流接收完成: {} 字节", received);

        Ok(())
    }
}

#[async_trait]
impl Input for RedisInput {
    async fn run(&self) -> Result<()> {
        tracing::info!(
            "启动 RedisInput: {:?}",
            self.config.redis.addresses
        );

        tracing::debug!("RedisInput 配置信息:");
        tracing::debug!("  - Redis 类型: {:?}", self.config.redis.redis_type);
        tracing::debug!("  - 认证类型: {:?}", self.config.redis.auth_type);
        tracing::debug!("  - TLS 启用: {}", self.config.redis.tls.enabled);
        tracing::debug!("  - RDB 并行数: {}", self.config.replay.rdb_parallel);
        tracing::debug!("  - 批量大小: {} 字节", self.config.replay.batch_size);
        tracing::debug!("  - 批量计数: {}", self.config.replay.batch_count);
        tracing::debug!("  - Run ID: {}", self.run_id);

        let mut stream = self.connect_redis().await?;

        self.authenticate(&mut stream).await?;

        // 发送 PSYNC 命令替代旧的 SYNC 命令
        self.send_psync_command(&mut stream).await?;

        let mut buf_reader = BufReader::new(stream);

        // 处理 PSYNC 响应（FULLRESYNC / CONTINUE）
        self.handle_psync_response(&mut buf_reader).await?;

        tracing::info!("RedisInput 数据读取完成");

        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        tracing::info!("停止 RedisInput");
        tracing::debug!("关闭 RedisInput 连接");
        Ok(())
    }
}
