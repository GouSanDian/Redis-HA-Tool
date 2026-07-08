//! syncer/syncer.rs - 同步器实现
//!
//! 本文件实现 SyncerImpl，管理同步任务的生命周期。

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::sync::Notify;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use async_trait::async_trait;
use crate::checkpoint::{CheckpointManager, CheckpointInfo};
use crate::config::{SyncConfig, DEFAULT_CHECKPOINT_INTERVAL_MS};
use crate::error::{SyncError, Result};
use crate::store::ReaderType;
use crate::syncer::{Syncer, SyncState, SyncRole, Input, Output, Channel, SyncFiniteStateMachine};

/// SyncerImpl - 同步器实现
///
/// 组装 Input、Output、状态机，管理同步生命周期。
pub struct SyncerImpl {
    /// 配置
    config: Arc<SyncConfig>,

    /// 当前状态（原子存储）
    state: AtomicU8,

    /// 当前角色
    role: AtomicU8,

    /// 同步状态机
    fsm: Arc<SyncFiniteStateMachine>,

    /// Input（可选）
    input: RwLock<Option<Arc<dyn Input>>>,

    /// Output（可选）
    output: RwLock<Option<Arc<dyn Output>>>,

    /// Channel（可选）
    channel: RwLock<Option<Arc<dyn Channel>>>,

    /// 取消令牌
    cancel_token: CancellationToken,

    /// Checkpoint 管理器（可选）
    ///
    /// 用于将同步进度写入目标 Redis 的 `redis_ha_tool_checkpoint` 和
    /// `redis_ha_tool_checkpoint_hash` 两个 Hash 结构中。
    checkpoint_manager: RwLock<Option<Arc<CheckpointManager>>>,

    /// 当前同步任务运行 ID
    run_id: RwLock<String>,

    /// Checkpoint 更新间隔
    checkpoint_interval: Duration,

    /// PSYNC 响应中获取的偏移量（共享给 Input 和转发任务）
    psync_offset: Arc<AtomicI64>,

    /// PSYNC 响应已就绪通知器
    psync_ready: Arc<Notify>,

    /// PSYNC 响应中获取的 master_replid（共享给 Input 和转发任务）
    psync_master_replid: Arc<Mutex<String>>,
}

impl SyncerImpl {
    /// 创建新的同步器
    ///
    /// # 参数
    /// - config: 同步配置
    pub fn new(config: Arc<SyncConfig>) -> Self {
        SyncerImpl {
            config,
            state: AtomicU8::new(SyncState::ReadyRun.as_u8()),
            role: AtomicU8::new(SyncRole::Leader.as_u8()),
            fsm: Arc::new(SyncFiniteStateMachine::new()),
            input: RwLock::new(None),
            output: RwLock::new(None),
            channel: RwLock::new(None),
            cancel_token: CancellationToken::new(),
            checkpoint_manager: RwLock::new(None),
            run_id: RwLock::new(String::new()),
            checkpoint_interval: Duration::from_millis(DEFAULT_CHECKPOINT_INTERVAL_MS),
            psync_offset: Arc::new(AtomicI64::new(0)),
            psync_ready: Arc::new(Notify::new()),
            psync_master_replid: Arc::new(Mutex::new(String::new())),
        }
    }

    /// 获取 PSYNC offset（共享引用，用于传递给 RedisInput）
    pub fn get_psync_offset(&self) -> Arc<AtomicI64> {
        self.psync_offset.clone()
    }

    /// 获取 PSYNC ready 通知器
    pub fn get_psync_ready(&self) -> Arc<Notify> {
        self.psync_ready.clone()
    }

    /// 获取 PSYNC master_replid（共享引用，用于传递给 RedisInput）
    pub fn get_psync_master_replid(&self) -> Arc<Mutex<String>> {
        self.psync_master_replid.clone()
    }

    /// 设置 Input
    pub async fn set_input(&self, input: Arc<dyn Input>) {
        *self.input.write().await = Some(input);
    }

    /// 设置 Output
    pub async fn set_output(&self, output: Arc<dyn Output>) {
        *self.output.write().await = Some(output);
    }

    /// 设置 Channel
    pub async fn set_channel(&self, channel: Arc<dyn Channel>) {
        *self.channel.write().await = Some(channel);
    }

    /// 设置 Checkpoint 管理器
    ///
    /// # 参数
    /// - manager: Checkpoint 管理器
    pub async fn set_checkpoint_manager(&self, manager: Arc<CheckpointManager>) {
        *self.checkpoint_manager.write().await = Some(manager);
    }

    /// 设置当前运行 ID
    ///
    /// # 参数
    /// - run_id: 运行 ID
    pub async fn set_run_id(&self, run_id: String) {
        *self.run_id.write().await = run_id;
    }

    /// 获取当前运行 ID
    pub async fn get_run_id(&self) -> String {
        self.run_id.read().await.clone()
    }

    /// 获取同步状态机
    pub fn fsm(&self) -> &Arc<SyncFiniteStateMachine> {
        &self.fsm
    }

    /// 获取取消令牌
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    /// 运行 Leader 模式
    async fn run_leader(&self) -> Result<()> {
        tracing::info!("启动 Leader 模式同步器");

        // 检查 Input 和 Output，并克隆 Arc 以便传递给异步任务
        let input_guard = self.input.read().await;
        let input = input_guard.as_ref()
            .ok_or_else(|| SyncError::Break("Input 未设置".into()))?
            .clone(); // 克隆 Arc

        let output_guard = self.output.read().await;
        let output = output_guard.as_ref()
            .ok_or_else(|| SyncError::Break("Output 未设置".into()))?
            .clone(); // 克隆 Arc

        tracing::debug!("Input 和 Output 已设置");

        // 启动状态机：Started -> FullInit
        self.fsm.start_full_sync();

        tracing::debug!("状态机已启动全量同步");

        // 克隆 cancel_token 以便传递给异步任务
        let cancel_token = self.cancel_token.clone();
        let fsm = self.fsm.clone();

        // 启动 Input 任务（并发运行）
        let input_task = tokio::spawn({
            async move {
                tracing::debug!("启动 Input 任务");
                tokio::select! {
                    result = input.run() => {
                        match result {
                            Ok(_) => {
                                tracing::debug!("Input 任务完成");
                                Ok(())
                            }
                            Err(e) => {
                                tracing::error!("Input 任务错误: {}", e);
                                Err(e)
                            }
                        }
                    }
                    _ = cancel_token.cancelled() => {
                        tracing::debug!("Input 任务收到取消信号");
                        input.stop().await.ok();
                        Err(SyncError::Break("Input 任务被取消".into()))
                    }
                }
            }
        });

        tracing::debug!("Input 任务已启动");

        // 推进状态机：FullInit -> FullSyncing
        fsm.begin_full_sync();

        tracing::debug!("状态机已进入全量同步进行中");

        let channel_guard = self.channel.read().await;
        let channel = channel_guard.as_ref()
            .ok_or_else(|| SyncError::Break("Channel 未设置".into()))?
            .clone();

        tracing::debug!("Channel 已设置，等待 PSYNC 响应确定起始偏移量");

        // 等待 PSYNC 响应（Input 端收到 FULLRESYNC/CONTINUE 后通知）
        // 这确保转发任务使用正确的初始 offset
        let psync_offset = self.psync_offset.clone();
        let psync_ready = self.psync_ready.clone();
        let psync_master_replid = self.psync_master_replid.clone();

        // 等待 PSYNC 就绪（超时防止死等）
        tokio::select! {
            _ = psync_ready.notified() => {
                tracing::debug!("PSYNC 响应已就绪");
            }
            _ = tokio::time::sleep(Duration::from_secs(30)) => {
                tracing::warn!("等待 PSYNC 响应超时（30s），使用默认 offset=0");
            }
        }

        let initial_offset = psync_offset.load(Ordering::Relaxed);
        tracing::info!("数据转发起始 offset: {}", initial_offset);

        let cancel_token2 = self.cancel_token.clone();
        let cancel_token3 = self.cancel_token.clone();
        let cancel_token4 = self.cancel_token.clone();
        let output_clone = output.clone();
        let channel_clone = channel.clone();
        // 为转发任务准备 checkpoint 信息
        let checkpoint_manager = self.checkpoint_manager.write().await.take();
        let checkpoint_interval = self.checkpoint_interval;

        let forwarding_task = tokio::spawn({
            async move {
                tracing::info!("启动数据转发任务（起始 offset: {}）", initial_offset);
                let mut offset = initial_offset;
                let mut persistent_stream: Option<TcpStream> = None;
                let notify = channel_clone.data_notify();
                let mut last_checkpoint_time = Instant::now();
                let mut rdb_forwarded = false;

                loop {
                    // 先在 select 外检查可用数据，避免 select 每次 poll 都重新执行
                    let available = match channel_clone.available_bytes(offset).await {
                        Ok(n) if n > 0 => n,
                        _ => 0,
                    };

                    if available == 0 {
                        // 无数据时等待通知（或取消信号），不会重复打印日志
                        tokio::select! {
                            _ = cancel_token3.cancelled() => {
                                tracing::info!("数据转发任务收到取消信号");
                                break;
                            }
                            _ = notify.notified() => {
                                continue;
                            }
                        }
                    }

                    // 有数据时进入 select 处理，同时响应取消信号
                    tokio::select! {
                        _ = cancel_token3.cancelled() => {
                            tracing::info!("数据转发任务收到取消信号");
                            break;
                        }

                        _ = async {
                            let reader = match channel_clone.get_reader(offset).await {
                                Ok(r) => r,
                                Err(e) => {
                                    tracing::debug!("无法获取 Reader (offset={}): {}", offset, e);
                                    notify.notified().await;
                                    return;
                                }
                            };

                            let reader_type = reader.reader_type();
                            let reader_size = reader.size();

                            match reader_type {
                                ReaderType::Rdb => {
                                    let rdb_size = reader_size.unwrap_or(0);
                                    tracing::info!("发送 RDB 数据: size={}, offset={}", rdb_size, offset);
                                    persistent_stream = None;
                                    match output_clone.send(reader).await {
                                        Ok(sent) => {
                                            offset += rdb_size;
                                            tracing::info!("RDB 转发完成，发送 {} 字节，下一个 offset={}", sent, offset);

                                            // RDB 转发完成后，保存初始 checkpoint
                                            if !rdb_forwarded {
                                                rdb_forwarded = true;
                                                if let Some(ref cm) = checkpoint_manager {
                                                    // 从 PSYNC 响应中获取 master_replid
                                                    // （PSYNC 就绪后该值已通过 FULLRESYNC/CONTINUE 响应设置）
                                                    let replid = psync_master_replid.lock().unwrap().clone();
                                                    if !replid.is_empty() {
                                                        // 使用 master_replid 作为字段前缀保存 checkpoint
                                                        let checkpoint = CheckpointInfo::new(
                                                            replid.clone(),
                                                            offset,
                                                        );
                                                        if let Err(e) = cm.save_checkpoint(&checkpoint).await {
                                                            tracing::warn!("保存 RDB checkpoint 失败: {}", e);
                                                        }

                                                        // 同时保存 PSYNC 信息（master_replid + offset）
                                                        // 以便下次启动时发送 PSYNC <replid> <offset> 实现增量同步
                                                        if let Err(e) = cm.save_psync_info(&replid, offset).await {
                                                            tracing::warn!("保存 PSYNC 信息失败: {}", e);
                                                        }
                                                    } else {
                                                        tracing::warn!("PSYNC master_replid 为空，跳过保存 checkpoint");
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            tracing::error!("发送 RDB 数据失败: {}", e);
                                            // 等待短暂时间后重试
                                            tokio::time::sleep(Duration::from_millis(100)).await;
                                        }
                                    }
                                }
                                ReaderType::Aof => {
                                    if persistent_stream.is_none() {
                                        match output_clone.new_stream().await {
                                            Ok(s) => {
                                                tracing::info!("AOF 持久连接已建立");
                                                persistent_stream = Some(s);
                                            }
                                            Err(e) => {
                                                tracing::error!("连接目标 Redis 失败: {}", e);
                                                tokio::time::sleep(Duration::from_millis(100)).await;
                                                return;
                                            }
                                        }
                                    }

                                    tracing::debug!("发送 AOF 数据: available={} bytes, offset={}", available, offset);
                                    match output_clone.send_with_stream(reader, persistent_stream.as_mut().unwrap()).await {
                                        Ok(sent) => {
                                            if sent > 0 {
                                                offset += sent;
                                                tracing::debug!("AOF 转发完成，发送 {} 字节，下一个 offset={}", sent, offset);

                                                // 定期保存 checkpoint
                                                if last_checkpoint_time.elapsed() >= checkpoint_interval {
                                                    if let Some(ref cm) = checkpoint_manager {
                                                        let replid = psync_master_replid.lock().unwrap().clone();
                                                        if !replid.is_empty() {
                                                            if let Err(e) = cm.update_offset(&replid, offset).await {
                                                                tracing::warn!("保存 AOF checkpoint 失败: {}", e);
                                                            } else {
                                                                tracing::debug!("保存 AOF checkpoint 成功: offset={}", offset);
                                                            }
                                                        }
                                                    }
                                                    last_checkpoint_time = Instant::now();
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            tracing::error!("发送 AOF 数据失败: {}，将重连", e);
                                            persistent_stream = None;
                                            tokio::time::sleep(Duration::from_millis(100)).await;
                                        }
                                    }
                                }
                            }
                        } => {}
                    }
                }

                tracing::info!("数据转发任务结束");
            }
        });

        tracing::debug!("数据转发任务已启动");

        // 等待 input_task 完成以推进 FSM 状态，但不终止 forwarding_task
        // 增量同步是持续进行的，forwarding_task 应一直运行直到收到取消信号
        let input_result = tokio::select! {
            _ = cancel_token2.cancelled() => {
                tracing::info!("收到取消信号，停止同步器");
                output.stop().await.ok();
                forwarding_task.abort();
                tracing::info!("Leader 模式同步器停止");
                return Ok(());
            }
            result = input_task => result,
        };

        match input_result {
            Ok(Ok(_)) => {
                tracing::debug!("Input 任务已完成");

                // 无论 FULLRESYNC 还是 CONTINUE，都推进状态机到增量同步阶段
                fsm.finish_full_sync();
                tracing::debug!("状态机已完成全量同步阶段");

                fsm.start_incr_sync();
                tracing::info!("进入增量同步阶段，数据转发任务持续运行中...");

                // 增量同步是持续进行的，不终止 forwarding_task
                // forwarding_task 会持续从 channel 读取数据并转发到目标 Redis
                // 只有收到取消信号时才停止
                cancel_token4.cancelled().await;
                tracing::info!("收到取消信号，停止数据转发任务");
                output.stop().await.ok();
                forwarding_task.abort();
            }
            Ok(Err(e)) => {
                tracing::error!("Input 任务失败: {}", e);
                forwarding_task.abort();
                return Err(e);
            }
            Err(e) => {
                tracing::error!("Input 任务 panicked: {}", e);
                forwarding_task.abort();
                return Err(SyncError::Break(format!("Input 任务 panicked: {}", e)));
            }
        }

        tracing::info!("Leader 模式同步器停止");

        Ok(())
    }

    /// 运行 Follower 模式
    async fn run_follower(&self) -> Result<()> {
        tracing::info!("启动 Follower 模式同步器");

        // Follower 模式从 Leader 接收数据
        // 需要实现 gRPC ReplicaFollower

        Ok(())
    }
}

impl SyncState {
    fn as_u8(&self) -> u8 {
        match self {
            SyncState::ReadyRun => 0,
            SyncState::Run => 1,
            SyncState::Pause => 2,
            SyncState::Stop => 3,
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            0 => SyncState::ReadyRun,
            1 => SyncState::Run,
            2 => SyncState::Pause,
            3 => SyncState::Stop,
            _ => SyncState::ReadyRun,
        }
    }
}

impl SyncRole {
    fn as_u8(&self) -> u8 {
        match self {
            SyncRole::Leader => 0,
            SyncRole::Follower => 1,
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            0 => SyncRole::Leader,
            1 => SyncRole::Follower,
            _ => SyncRole::Leader,
        }
    }
}

#[async_trait]
impl Syncer for SyncerImpl {
    async fn run(&self) -> Result<()> {
        // 检查状态
        let current_state = SyncState::from_u8(self.state.load(Ordering::SeqCst));
        if !current_state.can_run() {
            tracing::warn!("同步器状态不可运行: {:?}", current_state);
            return Err(SyncError::Break(format!("同步器状态不可运行: {:?}", current_state)));
        }

        // 设置状态为运行中
        self.state.store(SyncState::Run.as_u8(), Ordering::SeqCst);
        tracing::info!("同步器状态: Run");

        // 根据角色运行
        let role = SyncRole::from_u8(self.role.load(Ordering::SeqCst));
        let result = match role {
            SyncRole::Leader => self.run_leader().await,
            SyncRole::Follower => self.run_follower().await,
        };

        // 恢复状态
        self.state.store(SyncState::ReadyRun.as_u8(), Ordering::SeqCst);
        tracing::info!("同步器已停止");

        result
    }

    async fn stop(&self) -> Result<()> {
        tracing::info!("停止同步器");
        self.cancel_token.cancel();
        self.state.store(SyncState::Stop.as_u8(), Ordering::SeqCst);
        Ok(())
    }

    async fn pause(&self) -> Result<()> {
        tracing::info!("暂停同步器");
        self.state.store(SyncState::Pause.as_u8(), Ordering::SeqCst);
        Ok(())
    }

    async fn resume(&self) -> Result<()> {
        tracing::info!("恢复同步器");
        self.state.store(SyncState::ReadyRun.as_u8(), Ordering::SeqCst);
        Ok(())
    }

    fn status(&self) -> SyncState {
        SyncState::from_u8(self.state.load(Ordering::SeqCst))
    }

    fn role(&self) -> SyncRole {
        SyncRole::from_u8(self.role.load(Ordering::SeqCst))
    }
}
