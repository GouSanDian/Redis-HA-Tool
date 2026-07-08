# Redis Tool 软件详细设计文档（Rust 版）

## 目录

1. [概述](#1-概述)
2. [系统架构](#2-系统架构)
3. [核心组件设计](#3-核心组件设计)
4. [数据流设计](#4-数据流设计)
5. [协议设计](#5-协议设计)
6. [配置系统设计](#6-配置系统设计)
7. [存储系统设计](#7-存储系统设计)
8. [高可用设计](#8-高可用设计)
9. [过滤系统设计](#9-过滤系统设计)
10. [监控与运维设计](#10-监控与运维设计)
11. [接口设计](#11-接口设计)

---

## 1. 概述

### 1.1 项目简介

Redis Tool 是一个高性能的 Redis 数据同步工具，支持从源 Redis 实例实时同步数据到目标 Redis 实例。系统支持全量同步和增量同步，具备断点续传、双向同步、集群同步等高级特性。

### 1.2 核心特性

- **全量同步**：通过 RDB 快照实现完整数据迁移
- **增量同步**：通过 AOF 日志实现实时数据同步
- **断点续传**：支持从检查点恢复同步
- **双向同步**：通过 Circle Key 机制防止循环复制
- **集群支持**：支持 Redis Standalone、Sentinel、Cluster 三种模式
- **数据过滤**：支持按 DB、命令、Key 前缀、Slot 进行过滤
- **高可用**：支持 Leader-Follower 架构，自动故障转移
- **并行回放**：RDB 数据支持多 Task 并行写入目标 Redis

### 1.3 技术栈

| 类别 | 选型 | 说明 |
|------|------|------|
| **开发语言** | Rust 1.75+ (edition 2021) | 内存安全、零成本抽象 |
| **异步运行时** | Tokio | 高性能异步 I/O，多协程调度 |
| **HTTP 框架** | Axum | 基于 Tokio 的轻量级 Web 框架 |
| **RPC 框架** | tonic + prost | gRPC + Protobuf 的 Rust 实现 |
| **Redis 客户端** | redis-rs + 自定义 RESP 编解码器 | 支持 RESP2/RESP3 |
| **日志框架** | tracing + tracing-subscriber | 结构化日志，兼容 tokio 生态 |
| **日志轮转** | tracing-appender | 基于 tracing 的文件轮转 |
| **监控** | prometheus (prometheus-client) | Prometheus 指标暴露 |
| **序列化** | serde + serde_yaml + serde_json | YAML/JSON 配置解析 |
| **错误处理** | thiserror + anyhow | 库错误用 thiserror，应用错误用 anyhow |
| **服务发现** | etcd-client / redis | Leader 选举后端 |
| **端口复用** | tokio::net::TcpListener + 自定义多路复用 | gRPC + HTTP 共用端口 |
| **并发原语** | tokio::sync (mpsc, watch, oneshot, RwLock, Mutex) | 异步安全的并发控制 |

---

## 2. 系统架构

### 2.1 整体架构图

```
┌─────────────────────────────────────────────────────────────────┐
│                         Redis Tool                            │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────┐      ┌──────────┐      ┌──────────┐             │
│  │  Input   │─────>│ Channel  │─────>│  Output  │             │
│  │ (源Redis)│      │ (本地存储)│      │ (目标Redis)│            │
│  └──────────┘      └──────────┘      └──────────┘             │
│       ↑                  ↑                  ↑                   │
│       │                  │                  │                   │
│  ┌──────────┐      ┌──────────┐      ┌──────────┐             │
│  │  PSYNC   │      │  Store   │      │  Filter  │             │
│  │ Protocol │      │ Manager  │      │  Engine  │             │
│  └──────────┘      └──────────┘      └──────────┘             │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────┐      ┌──────────┐      ┌──────────┐             │
│  │  Cluster │      │ Checkpoint│     │  Metrics │             │
│  │ Election │      │ Manager  │      │ Collector│             │
│  └──────────┘      └──────────┘      └──────────┘             │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 核心架构模式

**三阶段管道架构**：

1. **Input 阶段**：从源 Redis 读取数据（PSYNC 协议）
2. **Channel 阶段**：本地文件存储（RDB + AOF）
3. **Output 阶段**：向目标 Redis 写入数据（RESP 协议）

这种设计实现了生产者和消费者的解耦，支持：
- 异步处理：Input 和 Output 作为独立的 Tokio Task 运行，互不阻塞
- 持久化：数据先落盘再回放，保证可靠性
- 断点续传：通过本地存储实现检查点机制

**Rust 并发模型**：

与 Go 的 goroutine + channel 不同，Rust 采用以下并发策略：

| Go 概念 | Rust 替代 | 说明 |
|---------|-----------|------|
| goroutine | `tokio::spawn` Task | 异步轻量级任务 |
| `chan` (unbuffered) | `tokio::sync::mpsc` | 异步通道，有界缓冲 |
| `chan` (buffered) | `tokio::sync::mpsc::channel(N)` | 有界异步通道 |
| `sync.WaitGroup` | `tokio::task::JoinSet` | 等待一组 Task 完成 |
| `context.Context` | `tokio_util::sync::CancellationToken` | 任务取消信号 |
| `sync.Mutex` | `tokio::sync::Mutex` | 跨 `.await` 点的互斥锁 |
| `sync.RWMutex` | `tokio::sync::RwLock` | 跨 `.await` 点的读写锁 |
| goroutine panic | `Result<T, E>` + `thiserror` | 编译期错误处理 |
| interface | `trait` | 特征（接口）定义 |
| struct embedding | 组合 + `Deref` / 手动委托 | 无继承，使用组合 |

---

## 3. 核心组件设计

### 3.1 Syncer（同步器）

**职责**：管理同步任务的生命周期和状态机

**核心状态**（`SyncState` 枚举）：
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncState {
    ReadyRun,
    Run,
    Pause,
    Stop,
}
```

**角色**（`SyncRole` 枚举）：
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncRole {
    Leader,    // 主同步器，负责从源 Redis 读取数据
    Follower,  // 从同步器，从 Leader 接收数据
}
```

**关键方法**：
```rust
#[async_trait]
pub trait Syncer: Send + Sync {
    async fn run(&self) -> Result<()>;
    async fn stop(&self) -> Result<()>;
    async fn pause(&self) -> Result<()>;
    async fn resume(&self) -> Result<()>;
    fn status(&self) -> SyncState;
}
```

- `run()`：启动同步器，根据角色分发到 `run_leader()` 或 `run_follower()`
- `run_leader()`：创建 Input、Output、ReplicaLeader，用 `JoinSet` 并行启动各 Task
- `run_follower()`：创建 ReplicaFollower，从 Leader 接收数据

**错误处理**（`SyncError` 枚举）：
```rust
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("syncer needs restart or exit: {0}")]
    Break(String),

    #[error("role change detected: {0}")]
    Role(String),

    #[error("data corrupted: {0}")]
    Corrupted(String),

    #[error("sync stopped by user")]
    StopSync,

    #[error("redis topology changed: {0}")]
    RedisTopologyChanged(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Redis(#[from] redis::RedisError),
}
```

### 3.2 RedisInput（数据输入）

**职责**：从源 Redis 读取数据

**核心流程**：
1. `sync_meta()`：确定全量/增量同步策略
2. `p_sync()`：发送 PSYNC 命令，协商同步模式
3. `sync_data()`：创建 RDB/AOF 写入器，开始数据同步
4. `start_sync_ack()`：用 `tokio::time::interval` 定期发送 REPLCONF ACK
5. `check_sync_delay()`：定期写入测试 key 测量延迟

**同步策略判断**（`sync_meta`）：
- 比较三个来源的 runId 和 offset：
  - 源 Redis（info replication）
  - 本地 Channel（`Storer::verify_run_id`）
  - 目标 Checkpoint（`CheckpointManager::get_checkpoint`）
- 四种情况：
  - 相同 runId + 有效 offset → 增量同步
  - 不同 runId 或无效 offset → 全量同步
  - 本地有数据但 checkpoint 无 → 全量同步
  - Checkpoint 有数据但本地无 → 尝试从 checkpoint 恢复

### 3.3 RedisOutput（数据输出）

**职责**：向目标 Redis 写入数据

**核心流程**：
1. `send()`：根据 Reader 类型分发到 `send_rdb()` 或 `send_aof()`
2. `send_rdb()`：解析 RDB，并行回放到目标 Redis
3. `send_aof()`：解析 AOF 命令，批量写入目标 Redis
4. `send_cmds_batch()`：主批处理循环，支持事务、pipeline、checkpoint 更新

**RDB 并行回放**：
- 使用 FNV hash 将 key 分发到 N 个并行 Tokio Task
- 每个 Task 持有独立的 `redis::aio::Connection`（通过连接池 `redis::aio::MultiplexedConnection`）
- 各 Task 通过 `tokio::sync::mpsc` 接收 `BinEntry`
- 应用过滤器（db、key 前缀、slot）

```rust
// RDB 并行回放示意
async fn send_rdb(&self, reader: &mut dyn Reader) -> Result<()> {
    let (senders, handles): (Vec<_>, Vec<_>) = (0..self.parallel)
        .map(|_| {
            let (tx, rx) = mpsc::channel::<BinEntry>(1024);
            let conn = self.pool.get().await?;
            let handle = tokio::spawn(Self::rdb_replay_worker(rx, conn, self.filter.clone()));
            Ok((tx, handle))
        })
        .collect::<Result<Vec<_>>>()?;

    // 分发 key 到不同 worker
    rdb::parse_rdb(reader, |entry| {
        let idx = fnv_hash(&entry.key) % senders.len();
        senders[idx].send(entry).await
    }).await?;

    // 等待所有 worker 完成
    drop(senders);
    for h in handles {
        h.await??;
    }
    Ok(())
}
```

**AOF 批处理**：
- 可配置批大小（batch size/count）
- 支持 pipeline 模式（不等待响应）
- 支持事务模式（multi/exec 包装）
- 定期更新 checkpoint

**Circle Key 机制**（双向同步）：
- 每个写命令生成 MD5 哈希，前缀为 `redis_ha_tool_circle_`
- 写入目标时同时写入 circle key
- 反向同步时检查 circle key 是否存在，存在则丢弃（防止循环）

### 3.4 StoreChannel（存储通道）

**职责**：桥接 Input 写入器和 Output 读取器

**实现**：封装 `store::Storer`，提供统一的 Channel trait

```rust
#[async_trait]
pub trait Channel: Send + Sync {
    async fn get_reader(&self, offset: i64) -> Result<Box<dyn Reader>>;
    async fn get_rdb_writer(&self, run_id: &str, offset: i64, size: i64)
        -> Result<Box<dyn AsyncWrite + Send + Unpin>>;
    async fn get_aof_writer(&self, run_id: &str, offset: i64)
        -> Result<Box<dyn AsyncWrite + Send + Unpin>>;
}
```

### 3.5 SyncFiniteStateMachine（同步状态机）

**状态转换**：
```
Started → FullInit → FullSyncing → FullSynced → IncrSyncing → IncrSynced
```

**核心方法**：
```rust
pub struct SyncFiniteStateMachine {
    state: AtomicU8,                              // 原子状态存储
    watchers: Vec<watch::Receiver<SyncPhase>>,    // 状态变化通知
    tx: watch::Sender<SyncPhase>,                 // 状态广播
}

impl SyncFiniteStateMachine {
    pub fn set_state(&self, state: SyncPhase);
    pub fn state_notify(&self) -> watch::Receiver<SyncPhase>;
}
```

使用 `tokio::sync::watch` 替代 Go 的 observer 模式，实现零拷贝的状态广播。

---

## 4. 数据流设计

### 4.1 全量同步流程

```
1. Input 发送 PSYNC <runId> <offset> 到源 Redis
2. 源响应 FULLRESYNC <runId> <offset>，开始 dump RDB
3. Input 读取 RDB 流，通过 Storer.get_rdb_writer() 写入本地文件
4. RDB 完成后，源继续发送增量 AOF 命令
5. Input 通过 Storer.get_aof_writer() 写入 AOF 文件
6. Output 通过 Storer.get_reader() 读取 RDB
7. Output 使用 rdb::parse_rdb() 解析，分发到并行 Task 回放
8. Output 读取 AOF 命令，批量发送到目标 Redis
```

### 4.2 增量同步流程

```
1. Input 发送 PSYNC <runId> <offset>（已知 runId 和 offset）
2. 源响应 CONTINUE，开始发送 AOF 命令
3. Input 写入 AOF 到本地存储
4. Output 读取并批量回放 AOF 命令
```

### 4.3 高可用流程（Leader/Follower）

```
1. 多个 syncer 实例运行，通过 etcd/Redis 选举 Leader
2. Leader 从源 Redis 读取数据，写入本地存储
3. Leader 通过 gRPC 流（tonic streaming）将数据发送给 Followers
4. Leader 故障时，Follower 竞选成为新 Leader
```

---

## 5. 协议设计

### 5.1 PSYNC 协议

**命令格式**：
```
PSYNC <runid> <offset>
```

**响应**：
- `FULLRESYNC <runid> <offset>`：需要全量同步
- `CONTINUE`：增量同步

**全量同步数据流**：
```
$<size>\r\n<RDB data><AOF stream>
```

**伴随命令**：
- `REPLCONF listening-port <port>`：设置监听端口
- `REPLCONF ack <offset>`：发送复制偏移量

### 5.2 RESP 协议（Redis Serialization Protocol）

**类型定义**：
- `+`：简单字符串
- `-`：错误
- `:`：整数
- `$`：批量字符串（带长度前缀）
- `*`：数组（带计数前缀）

**解码器实现**：`src/protocol/resp/decoder.rs`
- 使用 `bytes::BytesMut` 零拷贝缓冲
- 跟踪字节偏移量用于复制偏移计算
- 递归解码 RESP 类型

```rust
pub struct RespDecoder {
    buf: BytesMut,
    offset: u64,  // 复制偏移量
}

impl RespDecoder {
    pub fn decode(&mut self) -> Result<Option<RespValue>>;
    pub fn offset(&self) -> u64;
}

pub enum RespValue {
    SimpleString(Bytes),
    Error(Bytes),
    Integer(i64),
    BulkString(Bytes),
    Array(Vec<RespValue>),
    Null,
}
```

### 5.3 RDB 格式

**结构**：
- 签名："REDIS"（5 字节）
- 版本：4 字节
- AUX 字段（0xFA 标记）：键值元数据
- 数据库选择器、过期时间、键类型、值
- 校验和（RDB 版本 > 2）

**解析入口**：`src/protocol/rdb/parser.rs`
- 通过 `tokio::sync::mpsc` channel 发送 `BinEntry` 对象
- 使用 `bytes::Bytes` 避免数据拷贝

### 5.4 AOF 格式

**结构**：
- 标准 RESP 编码命令，连续存储
- 文件名：`{offset}.aof`
- 每个文件有头部（header_size 字节）
- 文件超过 `log_size` 时轮转

### 5.5 gRPC 协议（Peer Sync）

**Proto 定义**：`proto/api.proto`
```protobuf
service ApiService {
    rpc Sync (SyncRequest) returns (stream SyncResponse) {}
}
```

**消息类型**：
- `SyncRequest`：节点信息 + offset
- `SyncResponse`：
  - Code：META/CONTINUE/HANDOVER/CLEAR/FAULT/ERROR/FAILURE
  - Meta：run_id, msg, aof flag
  - offset, size, data

**Rust 代码生成**：通过 `tonic-build` 在 `build.rs` 中自动生成。

### 5.6 Checkpoint 格式

**存储位置**：目标 Redis 的 Hash 结构

**键名**：
- `redis_ha_tool_checkpoint`：checkpoint 名称
- `redis_ha_tool_checkpoint_hash`（DB 0）：master_replid → checkpointName 映射

**Hash 字段**：
- `<master_replid>`：字段名 = master_replid 的实际值（如 `5e2f1b3a...`），值 = 复制偏移量
- `<master_replid>_offset`：复制偏移量（与 `save_checkpoint` 保持一致）
- `<master_replid>_runid`：运行 ID
- `<master_replid>_version`：版本号
- `<master_replid>_mtime`：最后修改时间

**PSYNC 信息读取流程**（通过 HGETALL 扫描自动发现 master_replid）：

1. 不依赖任何固定字段名，直接 `HGETALL redis_ha_tool_checkpoint` 扫描所有字段
2. 过滤规则：字段名不以 `_offset`/`_runid`/`_version`/`_mtime` 结尾，且值可解析为数字 → 即为 master_replid → offset 映射
3. 取字典序最大的 master_replid（最新的），返回其与对应偏移量

*注：字段前缀从 `runId` 改为 `master_replid`，master_replid 可通过 `INFO REPLICATION` 命令获取。*

---

## 6. 配置系统设计

### 6.1 配置格式

配置文件支持两种格式：

1. **YAML 格式**：文件扩展名 `.yaml` 或 `.yml`
2. **JSON 格式**：文件扩展名 `.json` 或 `.jsonc`

系统根据文件扩展名自动判断格式进行解析。

### 6.2 配置结构

**顶层配置**（`SyncConfig`）：
```rust
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SyncConfig {
    pub license: LicenseConfig,
    pub server: ServerConfig,
    pub cluster: ClusterConfig,
    pub input: InputConfig,
    pub local_cache: LocalCacheConfig,
    pub output: OutputConfig,
    pub log: LogConfig,
}
```

**配置加载方法**：
```rust
impl SyncConfig {
    /// 从配置文件加载配置
    /// 
    /// 根据文件扩展名自动判断格式：
    /// - `.yaml` / `.yml`: YAML 格式
    /// - `.json` / `.jsonc`: JSON 格式
    pub fn from_file(path: &Path) -> Result<Self>;
    
    /// 从 YAML 文件加载配置
    pub fn from_yaml_file(path: &Path) -> Result<Self>;
    
    /// 从 JSON 文件加载配置
    pub fn from_json_file(path: &Path) -> Result<Self>;
}
```

### 6.3 关键配置项

**RedisConfig**：
```rust
#[derive(Debug, Deserialize, Clone)]
pub struct RedisConfig {
    pub addresses: Vec<String>,
    pub password: Option<String>,
    pub auth_type: Option<AuthType>,
    pub tls: Option<TlsConfig>,
    pub redis_type: RedisType,           // Standalone / Sentinel / Cluster
    pub slots: Option<Vec<SlotRange>>,
    pub cluster_shards: Option<Vec<String>>,
    pub keepalive: Option<KeepaliveConfig>,
    pub sel_node_strategy: Option<NodeSelectStrategy>,
}
```

**ReplayConfig**：
```rust
#[derive(Debug, Deserialize, Clone)]
pub struct ReplayConfig {
    pub resume_from_break_point: bool,
    pub key_exists: KeyExistsPolicy,     // None / Replace / Flush
    pub rdb_parallel: usize,
    pub pipeline: bool,
    pub transaction: bool,
    pub batch_size: usize,
    pub batch_count: usize,
}
```

**FilterConfig**：
```rust
#[derive(Debug, Deserialize, Clone)]
pub struct FilterConfig {
    pub db_black_list: Vec<u32>,
    pub cmd_black_list: Vec<String>,
    pub key_prefix_white_list: Vec<String>,
    pub key_prefix_black_list: Vec<String>,
    pub slot_white_list: Vec<SlotRange>,
    pub slot_black_list: Vec<SlotRange>,
}
```

### 6.3 常量定义

```rust
pub const CHECKPOINT_KEY: &str = "redis_ha_tool_checkpoint";
pub const CHECKPOINT_KEY_HASH_KEY: &str = "redis_ha_tool_checkpoint_hash";
pub const CIRCLE_PREFIX_KEY: &str = "redis_ha_tool_circle_";
pub const DELAY_PREFIX_KEY: &str = "redis_ha_tool_delay_";
pub const ELECTION_PREFIX_KEY: &str = "redis_ha_tool_input_election_";
```

---

## 7. 存储系统设计

### 7.1 Storer（存储管理器）

**职责**：管理本地文件存储，按 runId 组织目录

**核心方法**：
```rust
#[async_trait]
pub trait Storer: Send + Sync {
    async fn get_reader(&self, offset: i64) -> Result<Box<dyn Reader>>;
    async fn get_rdb_writer(&self, run_id: &str, offset: i64, size: i64)
        -> Result<Box<dyn AsyncWrite + Send + Unpin>>;
    async fn get_aof_writer(&self, run_id: &str, offset: i64)
        -> Result<Box<dyn AsyncWrite + Send + Unpin>>;
    async fn init_data_set(&self) -> Result<()>;
    async fn gc_data_set(&self) -> Result<()>;
    fn verify_run_id(&self, run_id: &str) -> bool;
}
```

**文件命名**：
- AOF：`{offset}.aof`
- RDB：`{offset}_{size}.rdb`

**实现要点**：
- 使用 `tokio::fs` 进行异步文件 I/O
- 使用 `tokio::io::BufWriter` 缓冲写入
- 文件元数据通过 `tokio::sync::RwLock<DataSet>` 管理

### 7.2 数据集管理

**内存结构**（`DataSet`）：
```rust
pub struct DataSet {
    rdb_files: Vec<DataSetRdb>,       // RDB 文件跟踪
    aof_segments: Vec<DataSetAof>,    // AOF 段跟踪（按 offset 有序）
}
```

**事件通知**：使用 `tokio::sync::watch` 或 `tokio::sync::broadcast` 替代 Go 的观察者模式，通知 Writer/Reader 生命周期事件。

---

## 8. 高可用设计

### 8.1 Leader 选举

**支持的后端**：
- etcd（`etcd-client` crate）
- Redis（`redis` crate）

**Trait 定义**：
```rust
#[async_trait]
pub trait Cluster: Send + Sync {
    async fn close(&self) -> Result<()>;
    async fn new_election(&self, key: &str) -> Result<Box<dyn Election>>;
    async fn register(&self, key: &str, value: &str) -> Result<()>;
    async fn discover(&self, key: &str) -> Result<Vec<String>>;
}

#[async_trait]
pub trait Election: Send + Sync {
    async fn renew(&self) -> Result<()>;
    async fn leader(&self) -> Result<String>;
    async fn campaign(&self, value: &str) -> Result<()>;
    async fn resign(&self) -> Result<()>;
}
```

### 8.2 Replica 机制

**ReplicaLeader**：
- 服务 Followers 的 gRPC 连接（tonic server streaming）
- 从本地 Channel 读取数据发送给 Followers

**ReplicaFollower**：
- 连接到 Leader（tonic client streaming）
- 接收数据写入本地 Channel

### 8.3 故障转移

- Leader 故障时，Follower 自动竞选
- 支持手动 handover/takeover
- 拓扑变化检测（shard 数量、master 变化、迁移）
- 使用 `CancellationToken` 实现优雅关闭和任务取消

---

## 9. 过滤系统设计

### 9.1 RedisKeyFilter

**过滤维度**：
- **DB 黑名单**：跳过指定 DB
- **命令黑名单**：使用 Trie 树匹配
- **Key 前缀白名单/黑名单**：使用 Trie 树匹配
- **Slot 白名单/黑名单**：使用 RangeList 匹配

**核心方法**：
```rust
pub struct RedisKeyFilter {
    db_black_list: HashSet<u32>,
    cmd_black_list: Trie,
    key_prefix_white_list: Trie,
    key_prefix_black_list: Trie,
    slot_white_list: RangeList,
    slot_black_list: RangeList,
}

impl RedisKeyFilter {
    pub fn filter_cmd_key(&self, cmd: &str, key: &[u8], db: u32) -> bool;
    pub fn command_key_positions(cmd: &str) -> Option<&[usize]>;
}
```

**特殊命令**：
- `NO_ROUTE_CMDS`：永不转发的命令（CLUSTER、AUTH、PSYNC 等）

### 9.2 Trie 树

**用途**：前缀匹配（命令、Key 前缀）

**实现**：`src/filter/trie.rs`

```rust
pub struct Trie {
    children: HashMap<u8, Box<Trie>>,
    is_end: bool,
}

impl Trie {
    pub fn insert(&mut self, key: &[u8]);
    pub fn starts_with(&self, prefix: &[u8]) -> bool;
    pub fn exact_match(&self, key: &[u8]) -> bool;
}
```

### 9.3 RangeList

**用途**：Slot 范围匹配

**实现**：`src/filter/range_list.rs`

```rust
pub struct RangeList {
    ranges: Vec<(u16, u16)>,  // (start, end) 有序不重叠
}

impl RangeList {
    pub fn contains(&self, slot: u16) -> bool;
    pub fn from_config(ranges: &[SlotRange]) -> Self;
}
```

---

## 10. 监控与运维设计

### 10.1 HTTP API

**框架**：Axum

**端点**：
- `GET /metrics`：Prometheus 指标
- `GET /health`：健康检查
- `GET /syncer/status`：同步状态
- `GET /syncer/config`：配置信息
- `POST /syncer/stop`：停止同步
- `POST /syncer/pause`：暂停同步
- `POST /syncer/resume`：恢复同步
- `POST /syncer/restart`：重启同步
- `POST /syncer/handover`：手动切换 Leader
- `POST /syncer/fullsync`：强制全量同步
- `POST /storage/gc`：存储垃圾回收
- `PUT /log_level`：动态调整日志级别
- `GET /dumpstack`：dump 当前任务栈信息
- `GET /debug/pprof`：性能分析（通过 `tikv-jemalloc-ctl` 或自定义）

**Axum 路由示例**：
```rust
fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/metrics", get(metrics_handler))
        .route("/health", get(health_handler))
        .route("/syncer/status", get(status_handler))
        .route("/syncer/stop", post(stop_handler))
        .route("/syncer/pause", post(pause_handler))
        .route("/syncer/resume", post(resume_handler))
        .with_state(state)
}
```

### 10.2 gRPC API

**服务**：`ApiService.Sync`
- 用于 Leader-Follower 数据同步
- 基于 tonic 实现 server streaming RPC

### 10.3 Prometheus 指标

**关键指标**：
- 同步延迟（`sync_delay_seconds`）
- RDB 回放进度（`rdb_replay_progress`）
- AOF 命令处理速率（`aof_commands_total`）
- 过滤统计（`filtered_commands_total`）
- 错误计数（`sync_errors_total`）

**实现**：使用 `prometheus-client` crate 注册和暴露指标。

### 10.4 日志系统

**框架**：tracing + tracing-subscriber + tracing-appender

**配置**：
```rust
use tracing_subscriber::{fmt, EnvFilter};
use tracing_appender::rolling;

fn init_logging(config: &LogConfig) {
    let file_appender = rolling::RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .max_log_files(config.max_backups)
        .filename_prefix("redis-ha-tool")
        .filename_suffix("log")
        .build(config.dir)
        .expect("failed to create log appender");

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.level));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(file_appender)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true)
        .json()  // JSON 格式输出
        .init();
}
```

**日志级别**：trace / debug / info / warn / error
**输出目标**：stdout / file
**日志轮转**：基于 `tracing-appender` 的 daily/hourly/minutely 轮转

---

## 11. 接口设计

### 11.1 Cmd trait

```rust
#[async_trait]
pub trait Cmd: Send + Sync {
    fn name(&self) -> &str;
    async fn start(&self) -> Result<()>;
    async fn stop(&self) -> Result<()>;
}
```

**实现**：`SyncerCmd`

### 11.2 Syncer trait

```rust
#[async_trait]
pub trait Syncer: Send + Sync {
    async fn run(&self) -> Result<()>;
    async fn stop(&self) -> Result<()>;
    async fn pause(&self) -> Result<()>;
    async fn resume(&self) -> Result<()>;
    fn status(&self) -> SyncState;
    async fn service_replica(
        &self,
        req: SyncRequest,
        stream: tonic::Streaming<SyncRequest>,
    ) -> Result<tonic::Response<Self::SyncStream>>;
}
```

### 11.3 Input trait

```rust
#[async_trait]
pub trait Input: Send + Sync {
    async fn run(&self) -> Result<()>;
    async fn stop(&self) -> Result<()>;
}
```

**实现**：`RedisInput`

### 11.4 Output trait

```rust
#[async_trait]
pub trait Output: Send + Sync {
    async fn send(&self, reader: Box<dyn Reader>) -> Result<()>;
    async fn stop(&self) -> Result<()>;
}
```

**实现**：`RedisOutput`

### 11.5 Channel trait

```rust
#[async_trait]
pub trait Channel: Send + Sync {
    async fn get_reader(&self, offset: i64) -> Result<Box<dyn Reader>>;
    async fn get_rdb_writer(
        &self, run_id: &str, offset: i64, size: i64,
    ) -> Result<Box<dyn AsyncWrite + Send + Unpin>>;
    async fn get_aof_writer(
        &self, run_id: &str, offset: i64,
    ) -> Result<Box<dyn AsyncWrite + Send + Unpin>>;
}
```

**实现**：`StoreChannel`

---

## 附录

### A. 关键数据结构

**BinEntry**（RDB 条目）：
```rust
pub struct BinEntry {
    pub db: u32,
    pub key: Bytes,
    pub value: Bytes,
    pub expire: Option<SystemTime>,
    pub rdb_type: RdbType,
}
```

**CheckpointInfo**：
```rust
pub struct CheckpointInfo {
    pub key: String,
    pub master_replid: String,
    pub offset: i64,
    pub version: u64,
    pub mtime: SystemTime,
}
```

**Transaction**（事务状态）：
```rust
pub struct Transaction {
    pub status: TransactionStatus,
    // No -> Barrier -> Begin -> In -> Commit
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionStatus {
    No,
    Barrier,
    Begin,
    In,
    Commit,
}
```

**RespValue**（RESP 协议值）：
```rust
pub enum RespValue {
    SimpleString(Bytes),
    Error(Bytes),
    Integer(i64),
    BulkString(Bytes),
    Array(Vec<RespValue>),
    Null,
}
```

### B. 外部依赖（Cargo.toml）

```toml
[dependencies]
# 异步运行时
tokio = { version = "1", features = ["full"] }
tokio-util = { version = "0.7", features = ["codec", "rt"] }
tokio-stream = "0.1"
futures = "0.3"
async-trait = "0.1"

# Web 框架
axum = "0.7"
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace"] }

# gRPC
tonic = "0.11"
prost = "0.12"

# Redis
redis = { version = "0.25", features = ["tokio-comp", "connection-manager", "cluster-async"] }

# 序列化
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"

# 日志
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
tracing-appender = "0.2"

# 监控
prometheus-client = "0.22"

# 错误处理
thiserror = "1"
anyhow = "1"

# 工具
bytes = "1"
md-5 = "0.10"
fnv = "1"
bytesize = "1"

# etcd（可选）
etcd-client = { version = "0.12", optional = true }

# 内存分配器
tikv-jemallocator = { version = "0.5", optional = true }

[build-dependencies]
tonic-build = "0.11"
```

### C. 文件结构

```
redis-ha-tool-rust/
├── Cargo.toml                  # 工作空间 / 单 crate 配置
├── Cargo.lock
├── build.rs                    # tonic-build 代码生成
├── config/
│   └── config.json             # 示例配置
├── proto/
│   └── api.proto               # gRPC 定义
├── src/
│   ├── main.rs                 # 入口，CLI 解析，启动服务
│   ├── lib.rs                  # 库根，模块声明
│   ├── config/
│   │   ├── mod.rs              # 配置类型定义
│   │   └── constants.rs        # 常量
│   ├── cmd/
│   │   ├── mod.rs              # Cmd trait
│   │   ├── syncer_cmd.rs       # SyncerCmd 实现
│   │   └── api.rs              # HTTP/gRPC API 路由
│   ├── syncer/
│   │   ├── mod.rs              # Syncer trait + 导出
│   │   ├── syncer.rs           # Syncer 实现
│   │   ├── input.rs            # RedisInput
│   │   ├── output.rs           # RedisOutput
│   │   ├── channel.rs          # StoreChannel
│   │   ├── state_machine.rs    # 状态机
│   │   ├── replica.rs          # Leader/Follower
│   │   └── transaction.rs      # 事务跟踪
│   ├── protocol/
│   │   ├── mod.rs
│   │   ├── resp/
│   │   │   ├── mod.rs
│   │   │   ├── decoder.rs      # RESP 解码器
│   │   │   ├── encoder.rs      # RESP 编码器
│   │   │   └── value.rs        # RespValue 类型
│   │   └── rdb/
│   │       ├── mod.rs
│   │       ├── parser.rs       # RDB 解析器
│   │       └── types.rs        # RDB 类型常量
│   ├── store/
│   │   ├── mod.rs              # Storer trait + Reader trait
│   │   ├── storer.rs           # 文件存储实现
│   │   ├── reader.rs           # RDB/AOF Reader
│   │   ├── writer.rs           # RDB/AOF Writer
│   │   └── dataset.rs          # 数据集管理
│   ├── filter/
│   │   ├── mod.rs              # Filter trait
│   │   ├── key_filter.rs       # RedisKeyFilter
│   │   ├── trie.rs             # Trie 树
│   │   └── range_list.rs       # Slot 范围列表
│   ├── cluster/
│   │   ├── mod.rs              # Cluster/Election trait
│   │   ├── etcd.rs             # etcd 实现
│   │   └── redis_election.rs   # Redis 选举实现
│   ├── checkpoint/
│   │   ├── mod.rs              # CheckpointManager
│   │   └── manager.rs
│   ├── metric/
│   │   ├── mod.rs              # Prometheus 指标注册
│   │   └── collector.rs        # 指标收集
│   ├── error.rs                # 全局错误类型定义
│   └── utils/
│       ├── mod.rs
│       └── hash.rs             # FNV hash 等工具
└── tools/
    └── license_gen/            # License 生成工具
        ├── Cargo.toml
        └── src/main.rs
```

### D. 关键流程时序图

**全量同步时序**：
```
Input          Store          Output         Source Redis    Target Redis
  |              |              |              |              |
  |--PSYNC------>|              |              |              |
  |              |              |              |--FULLRESYNC->|
  |<-------------|--------------|--------------|              |
  |--RDB data--->|              |              |              |
  |              |--Read RDB--->|              |              |
  |              |              |--Parse RDB-->|              |
  |              |              |--Write------>|              |
  |--AOF cmd---->|              |              |              |
  |              |--Read AOF--->|              |              |
  |              |              |--Batch------->|              |
```

### E. Rust 特有的设计考量

#### E.1 所有权与生命周期

- **`Bytes` 类型**：RDB/AOF 数据使用 `bytes::Bytes` 实现零拷贝共享，避免 `Vec<u8>` 的拷贝开销
- **`Arc` 共享**：Filter、Config 等不可变共享数据使用 `Arc<T>` 包装
- **生命周期标注**：Reader/Writer 使用 `Box<dyn Trait + Send + 'a>` 管理生命周期

#### E.2 错误传播

- 使用 `?` 运算符 + `thiserror` 定义领域错误
- 应用层使用 `anyhow::Result` 简化错误处理
- 跨 Task 错误传播使用 `JoinError` 或 `oneshot::channel`

#### E.3 异步安全

- 所有 trait 方法标注 `async` + `Send` bound
- 使用 `#[async_trait]` 宏简化异步 trait 定义（Rust 1.75+ 可考虑原生 `async fn in trait`）
- 避免在持有 `MutexGuard` 时 `.await`（使用 `tokio::sync::Mutex` 或缩小临界区）

#### E.4 内存安全

- 无 `unsafe` 代码（除 jemalloc 绑定外）
- 使用 `bytes::Bytes` 的安全引用计数替代裸指针
- 编译期防止数据竞争（Send/Sync 约束）

#### E.5 性能优化

- **零拷贝解析**：RESP/RDB 解析基于 `BytesMut`，避免不必要的内存分配
- **连接池**：使用 `redis::aio::MultiplexedConnection` 复用连接
- **批处理**：AOF 命令批量发送，减少系统调用
- **并行回放**：RDB 使用 FNV hash 分片到多个 Tokio Task
- **jemalloc**：可选使用 jemalloc 替代系统分配器，提升多线程分配性能

---

**文档版本**：2.0（Rust 版）
**最后更新**：2026-07-06
**作者**：AI Assistant
