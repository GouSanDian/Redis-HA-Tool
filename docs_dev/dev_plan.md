# Redis Tool 开发计划

> 基于 [DESIGN.md](./docs/DESIGN.md) 编写，分 4 个阶段递进交付。  
> 每个阶段功能独立完整，可单独编译、测试、验证。

---

## 总体里程碑

| 阶段 | 名称 | 核心目标 | 预估工期 |
|------|------|----------|----------|
| Phase 1 | 基础设施层 | 项目骨架、配置系统、RESP 协议、日志系统 | 2 周 |
| Phase 2 | 存储与过滤层 | 本地存储引擎、过滤系统、Checkpoint 管理 | 2 周 |
| Phase 3 | 同步引擎层 | Input/Output 管道、Syncer 状态机、RDB 解析回放 | 3 周 |
| Phase 4 | 高可用与运维层 | 集群选举、gRPC 副本、HTTP API、Prometheus 监控 | 2 周 |

---

## 阶段一：基础设施层

### 1.1 目标

搭建项目骨架，实现配置加载、RESP 协议编解码、日志系统、错误类型定义。  
本阶段完成后，项目可编译运行，能解析和生成 Redis RESP 协议报文，能加载 Json 配置。

### 1.2 任务清单

#### 1.2.1 项目骨架搭建

| 序号 | 任务 | 涉及文件 | 说明 |
|------|------|----------|------|
| 1 | 初始化 Cargo 项目 | `Cargo.toml` | 声明所有依赖（tokio, bytes, serde, tracing 等） |
| 2 | 创建目录结构 | `src/` 各子目录 | 按 DESIGN.md 附录 C 创建模块目录 |
| 3 | 编写 `lib.rs` | `src/lib.rs` | 声明所有子模块 |
| 4 | 编写 `main.rs` 骨架 | `src/main.rs` | 初始化 tracing、加载配置、打印启动信息 |
| 5 | 编写 `build.rs` | `build.rs` | tonic-build 代码生成（此阶段先留空壳） |

#### 1.2.2 错误类型定义

| 序号 | 任务 | 涉及文件 | 说明 |
|------|------|----------|------|
| 1 | 定义全局错误枚举 | `src/error.rs` | `SyncError` 枚举，使用 thiserror |
| 2 | 定义 `Result` 别名 | `src/error.rs` | `pub type Result<T> = std::result::Result<T, SyncError>` |

#### 1.2.3 配置系统

| 序号 | 任务 | 涉及文件 | 说明 |
|------|------|----------|------|
| 1 | 定义配置结构体 | `src/config/mod.rs` | `SyncConfig`, `RedisConfig`, `ReplayConfig`, `FilterConfig` 等 |
| 2 | 定义常量 | `src/config/constants.rs` | `CHECKPOINT_KEY`, `CIRCLE_PREFIX_KEY` 等 |
| 3 | 实现配置加载 | `src/config/mod.rs` | `SyncConfig::from_file(path)` 方法 |
| 4 | 编写示例配置 | `config/config.json` | 完整的 Json 示例配置文件 |
| 5 | 配置单元测试 | `src/config/mod.rs` | 测试 Json 反序列化、默认值、边界值 |

#### 1.2.4 RESP 协议编解码

| 序号 | 任务 | 涉及文件 | 说明 |
|------|------|----------|------|
| 1 | 定义 `RespValue` 枚举 | `src/protocol/resp/value.rs` | SimpleString/Error/Integer/BulkString/Array/Null |
| 2 | 实现 RESP 编码器 | `src/protocol/resp/encoder.rs` | `RespEncoder::encode(value) -> BytesMut` |
| 3 | 实现 RESP 解码器 | `src/protocol/resp/decoder.rs` | `RespDecoder::decode() -> Option<RespValue>`，跟踪 offset |
| 4 | RESP 编解码单元测试 | 各文件内 `#[cfg(test)]` | 覆盖所有类型、边界情况、不完整数据 |

#### 1.2.5 日志系统

| 序号 | 任务 | 涉及文件 | 说明 |
|------|------|----------|------|
| 1 | 实现日志初始化 | `src/utils/log.rs` | `init_logging(config)` 函数 |
| 2 | 支持文件/stdout 输出 | `src/utils/log.rs` | 基于 tracing-subscriber |
| 3 | 支持动态日志级别 | `src/utils/log.rs` | 通过 reload handle 运行时调整 |

#### 1.2.6 工具模块

| 序号 | 任务 | 涉及文件 | 说明 |
|------|------|----------|------|
| 1 | FNV hash 工具 | `src/utils/hash.rs` | `fnv_hash(data: &[u8]) -> u64` |
| 2 | 工具函数单元测试 | `src/utils/hash.rs` | 基础 hash 验证 |

### 1.3 交付物

- 可编译运行的二进制程序（启动后加载配置、初始化日志、打印版本信息后退出）
- RESP 编解码库（可独立使用）
- 配置加载库（可解析 Json）
- 全部单元测试通过

### 1.4 验证方法

```bash
# 编译
cargo build

# 运行单元测试
cargo test

# 运行程序验证启动
cargo run -- --config config/config.json
# 预期输出：版本信息、配置加载成功日志
```

---

## 阶段二：存储与过滤层

### 2.1 目标

实现本地文件存储引擎（Storer/Reader/Writer）、过滤系统（Trie/RangeList/RedisKeyFilter）、Checkpoint 管理。  
本阶段完成后，系统可以管理 RDB/AOF 文件的读写，可以按规则过滤 Redis 命令和 Key。

### 2.2 任务清单

#### 2.2.1 存储系统

| 序号 | 任务 | 涉及文件 | 说明 |
|------|------|----------|------|
| 1 | 定义 `Reader` trait | `src/store/mod.rs` | 支持 RDB/AOF 两种 Reader 类型 |
| 2 | 定义 `Storer` trait | `src/store/mod.rs` | `get_reader`, `get_rdb_writer`, `get_aof_writer` 等 |
| 3 | 实现 `DataSet` | `src/store/dataset.rs` | RDB/AOF 文件元数据跟踪 |
| 4 | 实现 `FileStorer` | `src/store/storer.rs` | 基于本地文件系统的 Storer 实现 |
| 5 | 实现 `RdbReader` | `src/store/reader.rs` | 读取 RDB 文件，支持按 offset 定位 |
| 6 | 实现 `AofReader` | `src/store/reader.rs` | 读取 AOF 文件，支持跨段读取 |
| 7 | 实现 `RdbWriter` | `src/store/writer.rs` | 写入 RDB 文件 |
| 8 | 实现 `AofWriter` | `src/store/writer.rs` | 写入 AOF 文件，支持文件轮转 |
| 9 | 实现 `StoreChannel` | `src/syncer/channel.rs` | 桥接 Storer 的 Channel trait 实现 |
| 10 | 存储系统单元测试 | 各文件内 `#[cfg(test)]` | 文件创建/读写/轮转/GC 测试 |

#### 2.2.2 过滤系统

| 序号 | 任务 | 涉及文件 | 说明 |
|------|------|----------|------|
| 1 | 实现 `Trie` 树 | `src/filter/trie.rs` | insert / starts_with / exact_match |
| 2 | 实现 `RangeList` | `src/filter/range_list.rs` | Slot 范围匹配 |
| 3 | 定义命令 Key 位置表 | `src/filter/key_filter.rs` | `command_key_positions()` 函数 |
| 4 | 定义 `NO_ROUTE_CMDS` | `src/filter/key_filter.rs` | 永不转发的命令集合 |
| 5 | 实现 `RedisKeyFilter` | `src/filter/key_filter.rs` | 多维度过滤逻辑 |
| 6 | 过滤系统单元测试 | 各文件内 `#[cfg(test)]` | Trie/RangeList/Filter 各维度测试 |

#### 2.2.3 Checkpoint 管理

| 序号 | 任务 | 涉及文件 | 说明 |
|------|------|----------|------|
| 1 | 定义 `CheckpointInfo` | `src/checkpoint/mod.rs` | Checkpoint 数据结构 |
| 2 | 实现 `CheckpointManager` | `src/checkpoint/manager.rs` | 读写 Checkpoint 到目标 Redis |
| 3 | Checkpoint 单元测试 | 各文件内 `#[cfg(test)]` | 使用 mock Redis 连接测试 |

### 2.3 交付物

- 本地文件存储引擎（可管理 RDB/AOF 文件生命周期）
- 过滤引擎（可按 DB/命令/Key 前缀/Slot 过滤）
- Checkpoint 管理器（可读写 Redis Hash 中的检查点信息）
- 全部单元测试通过

### 2.4 验证方法

```bash
# 运行单元测试
cargo test

# 存储系统验证（集成测试）
cargo test --test test_store
# 测试内容：
#   - 创建 RDB/AOF Writer 并写入数据
#   - 通过 Reader 读取并验证内容
#   - 文件轮转验证
#   - GC 清理旧文件验证

# 过滤系统验证
cargo test --test test_filter
# 测试内容：
#   - Trie 树前缀匹配
#   - RangeList Slot 匹配
#   - RedisKeyFilter 多维度过滤
```

---

## 阶段三：同步引擎层

### 3.1 目标

实现完整的同步管道：RedisInput（PSYNC 协议）、RedisOutput（RDB 回放 + AOF 批处理）、Syncer 状态机。  
本阶段完成后，系统可以从源 Redis 全量/增量同步数据到目标 Redis，是核心功能阶段。

### 3.2 任务清单

#### 3.2.1 RDB 解析器

| 序号 | 任务 | 涉及文件 | 说明 |
|------|------|----------|------|
| 1 | 定义 RDB 类型常量 | `src/protocol/rdb/types.rs` | RDB 数据类型枚举 |
| 2 | 定义 `BinEntry` | `src/protocol/rdb/mod.rs` | RDB 条目结构体 |
| 3 | 实现 RDB 解析器 | `src/protocol/rdb/parser.rs` | 解析 RDB 文件，通过回调发送 BinEntry |
| 4 | RDB 解析单元测试 | `src/protocol/rdb/parser.rs` | 使用预生成的 RDB 文件测试 |

#### 3.2.2 RedisInput（数据输入）

| 序号 | 任务 | 涉及文件 | 说明 |
|------|------|----------|------|
| 1 | 定义 `Input` trait | `src/syncer/mod.rs` | `run()` / `stop()` |
| 2 | 实现 `RedisInput` 结构体 | `src/syncer/input.rs` | 持有源 Redis 连接、Channel、配置 |
| 3 | 实现 `sync_meta()` | `src/syncer/input.rs` | 判断全量/增量同步策略 |
| 4 | 实现 `p_sync()` | `src/syncer/input.rs` | 发送 PSYNC 命令，处理 FULLRESYNC/CONTINUE |
| 5 | 实现 `sync_data()` | `src/syncer/input.rs` | 创建 Writer，读取并写入 RDB/AOF 数据 |
| 6 | 实现 `start_sync_ack()` | `src/syncer/input.rs` | 定期发送 REPLCONF ACK |
| 7 | 实现 `check_sync_delay()` | `src/syncer/input.rs` | 延迟测量 |
| 8 | Input 单元测试 | `src/syncer/input.rs` | 使用 mock 连接测试协议交互 |

#### 3.2.3 RedisOutput（数据输出）

| 序号 | 任务 | 涉及文件 | 说明 |
|------|------|----------|------|
| 1 | 定义 `Output` trait | `src/syncer/mod.rs` | `send()` / `stop()` |
| 2 | 实现 `RedisOutput` 结构体 | `src/syncer/output.rs` | 持有目标 Redis 连接池、Filter、配置 |
| 3 | 实现 `send_rdb()` | `src/syncer/output.rs` | RDB 并行回放（多 Task + FNV hash 分片） |
| 4 | 实现 `send_aof()` | `src/syncer/output.rs` | AOF 命令读取与批处理 |
| 5 | 实现 `send_cmds_batch()` | `src/syncer/output.rs` | 批量发送，支持 pipeline/事务模式 |
| 6 | 实现 Circle Key 机制 | `src/syncer/output.rs` | 双向同步防循环 |
| 7 | Output 单元测试 | `src/syncer/output.rs` | 使用 mock 连接测试回放逻辑 |

#### 3.2.4 Syncer 状态机与生命周期

| 序号 | 任务 | 涉及文件 | 说明 |
|------|------|----------|------|
| 1 | 定义 `SyncState` / `SyncRole` | `src/syncer/mod.rs` | 枚举类型 |
| 2 | 定义 `Syncer` trait | `src/syncer/mod.rs` | run/stop/pause/resume/status |
| 3 | 实现 `SyncFiniteStateMachine` | `src/syncer/state_machine.rs` | 状态转换 + watch 通知 |
| 4 | 实现 `SyncerImpl` | `src/syncer/syncer.rs` | 组装 Input/Output，管理生命周期 |
| 5 | 实现 `run_leader()` | `src/syncer/syncer.rs` | 启动 Input + Output Task |
| 6 | 实现事务跟踪 | `src/syncer/transaction.rs` | MULTI/EXEC 状态机 |
| 7 | Syncer 单元测试 | 各文件内 `#[cfg(test)]` | 状态机转换、生命周期测试 |

#### 3.2.5 Cmd 层

| 序号 | 任务 | 涉及文件 | 说明 |
|------|------|----------|------|
| 1 | 定义 `Cmd` trait | `src/cmd/mod.rs` | name/start/stop |
| 2 | 实现 `SyncerCmd` | `src/cmd/syncer_cmd.rs` | 封装 Syncer，提供命令行入口 |
| 3 | 完善 `main.rs` | `src/main.rs` | 解析 CLI 参数，创建并启动 SyncerCmd |

### 3.3 交付物

- 完整的 RDB 解析器
- RedisInput：可从源 Redis 通过 PSYNC 拉取数据
- RedisOutput：可将数据回放到目标 Redis（支持 RDB 并行回放 + AOF 批处理）
- Syncer 状态机：管理同步生命周期
- 端到端同步能力（Standalone 模式）

### 3.4 验证方法

```bash
# 运行全部单元测试
cargo test

# 集成测试：全量同步
cargo test --test test_full_sync
# 测试环境：
#   - 源 Redis (127.0.0.1:6379) 预填充测试数据
#   - 目标 Redis (127.0.0.1:6380) 清空
# 验证：
#   - 目标 Redis 中数据与源一致
#   - RDB 文件正确生成
#   - 各数据类型（string/hash/list/set/zset）正确同步

# 集成测试：增量同步
cargo test --test test_incr_sync
# 验证：
#   - 全量同步完成后，源端新增数据能实时同步到目标端
#   - 延迟在可接受范围内

# 集成测试：断点续传
cargo test --test test_resume
# 验证：
#   - 同步中途停止后重启，能从 checkpoint 恢复
#   - 数据不丢失不重复

# 集成测试：过滤功能
cargo test --test test_filter_integration
# 验证：
#   - DB 黑名单生效
#   - Key 前缀过滤生效
#   - 命令黑名单生效
```

---

## 阶段四：高可用与运维层

### 4.1 目标

实现集群选举、gRPC 副本同步、HTTP 管理 API、Prometheus 监控。  
本阶段完成后，系统具备生产级运维能力，支持高可用部署。

### 4.2 任务清单

#### 4.2.1 gRPC 协议定义

| 序号 | 任务 | 涉及文件 | 说明 |
|------|------|----------|------|
| 1 | 编写 proto 文件 | `proto/api.proto` | ApiService 定义 |
| 2 | 配置 build.rs | `build.rs` | tonic-build 编译 proto |
| 3 | 验证代码生成 | `src/api/` | 确认生成的 Rust 代码可用 |

#### 4.2.2 集群选举

| 序号 | 任务 | 涉及文件 | 说明 |
|------|------|----------|------|
| 1 | 定义 `Cluster` trait | `src/cluster/mod.rs` | close/new_election/register/discover |
| 2 | 定义 `Election` trait | `src/cluster/mod.rs` | renew/leader/campaign/resign |
| 3 | 实现 Redis 选举 | `src/cluster/redis_election.rs` | 基于 Redis SET NX + TTL |
| 4 | 实现 etcd 选举 | `src/cluster/etcd.rs` | 基于 etcd lease + election |
| 5 | 集群选举单元测试 | 各文件内 `#[cfg(test)]` | mock 后端测试选举逻辑 |

#### 4.2.3 Replica 机制

| 序号 | 任务 | 涉及文件 | 说明 |
|------|------|----------|------|
| 1 | 实现 `ReplicaLeader` | `src/syncer/replica.rs` | gRPC server streaming 发送数据 |
| 2 | 实现 `ReplicaFollower` | `src/syncer/replica.rs` | gRPC client 接收数据写入本地 Channel |
| 3 | 实现故障转移逻辑 | `src/syncer/replica.rs` | Leader 故障 → Follower 竞选 |
| 4 | 实现 `run_follower()` | `src/syncer/syncer.rs` | Follower 模式启动 |
| 5 | Replica 单元测试 | `src/syncer/replica.rs` | mock gRPC 流测试 |

#### 4.2.4 HTTP 管理 API

| 序号 | 任务 | 涉及文件 | 说明 |
|------|------|----------|------|
| 1 | 定义 `AppState` | `src/cmd/api.rs` | 共享状态（Syncer handle、Config 等） |
| 2 | 实现 `/health` | `src/cmd/api.rs` | 健康检查 |
| 3 | 实现 `/syncer/status` | `src/cmd/api.rs` | 同步状态查询 |
| 4 | 实现 `/syncer/stop` | `src/cmd/api.rs` | 停止同步 |
| 5 | 实现 `/syncer/pause` | `src/cmd/api.rs` | 暂停同步 |
| 6 | 实现 `/syncer/resume` | `src/cmd/api.rs` | 恢复同步 |
| 7 | 实现 `/syncer/restart` | `src/cmd/api.rs` | 重启同步 |
| 8 | 实现 `/syncer/fullsync` | `src/cmd/api.rs` | 强制全量同步 |
| 9 | 实现 `/syncer/handover` | `src/cmd/api.rs` | 手动切换 Leader |
| 10 | 实现 `/storage/gc` | `src/cmd/api.rs` | 存储 GC |
| 11 | 实现 `/log_level` | `src/cmd/api.rs` | 动态日志级别 |
| 12 | 实现 `/dumpstack` | `src/cmd/api.rs` | 任务栈 dump |
| 13 | HTTP API 单元测试 | `src/cmd/api.rs` | 使用 axum::test 测试 |

#### 4.2.5 Prometheus 监控

| 序号 | 任务 | 涉及文件 | 说明 |
|------|------|----------|------|
| 1 | 定义指标注册表 | `src/metric/mod.rs` | 全局 MetricRegistry |
| 2 | 实现指标收集器 | `src/metric/collector.rs` | sync_delay, rdb_progress, aof_commands_total 等 |
| 3 | 实现 `/metrics` 端点 | `src/cmd/api.rs` | 暴露 Prometheus 格式指标 |
| 4 | 在 Input/Output 中埋点 | `src/syncer/input.rs`, `output.rs` | 关键路径指标上报 |
| 5 | 监控单元测试 | `src/metric/` | 指标注册和读取测试 |

#### 4.2.6 集群模式支持

| 序号 | 任务 | 涉及文件 | 说明 |
|------|------|----------|------|
| 1 | 实现 Sentinel 模式连接 | `src/syncer/input.rs` | 通过 Sentinel 发现 master |
| 2 | 实现 Cluster 模式连接 | `src/syncer/input.rs` | 多 shard 并行 PSYNC |
| 3 | 拓扑变化检测 | `src/syncer/input.rs` | shard 数量/master 变化检测 |
| 4 | 集群模式集成测试 | `tests/` | 多节点环境验证 |

### 4.3 交付物

- gRPC 副本同步（Leader → Follower）
- 集群选举（Redis/etcd 后端）
- HTTP 管理 API（13 个端点）
- Prometheus 指标暴露
- Sentinel / Cluster 模式支持
- 生产级可部署版本

### 4.4 验证方法

```bash
# 运行全部单元测试
cargo test

# 集成测试：Leader-Follower 同步
cargo test --test test_replica
# 验证：
#   - Leader 正常同步数据
#   - Follower 通过 gRPC 接收数据
#   - Leader 故障后 Follower 自动竞选

# 集成测试：HTTP API
cargo test --test test_http_api
# 验证：
#   - /health 返回 200
#   - /syncer/status 返回正确状态
#   - /syncer/pause + /syncer/resume 控制同步
#   - /syncer/stop 停止同步
#   - /metrics 返回 Prometheus 格式

# 集成测试：集群选举
cargo test --test test_election
# 验证：
#   - Redis 后端选举正常
#   - Leader 续期正常
#   - Leader 故障后重新选举

# 端到端验证（手动）
# 1. 启动源 Redis + 目标 Redis
# 2. 启动 syncer（配置 HA 模式，2 个实例）
# 3. 验证 Leader 同步数据
# 4. kill Leader，验证 Follower 接管
# 5. 访问 HTTP API 验证状态
# 6. 访问 /metrics 验证指标
```

---

## 阶段一功能性测试文档

### 测试概述

| 项目 | 内容 |
|------|------|
| **测试阶段** | Phase 1 - 基础设施层 |
| **测试范围** | 配置系统、RESP 协议编解码、日志系统、工具函数 |
| **测试类型** | 单元测试 |
| **前置条件** | Rust 1.75+ 工具链已安装 |

### 测试用例

#### TC-1.1 配置系统测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-1.1.1 | 加载完整配置文件 | 包含所有字段的 config.json | 成功解析，所有字段值正确 |
| TC-1.1.2 | 加载最小配置文件 | 仅包含必填字段的 config.json | 成功解析，可选字段为默认值 |
| TC-1.1.3 | 加载不存在的配置文件 | 不存在的文件路径 | 返回 Io 错误 |
| TC-1.1.4 | 加载格式错误的配置文件 | 非法 Json 内容 | 返回解析错误 |
| TC-1.1.5 | Redis 类型解析 | `standalone` / `sentinel` / `cluster` | 正确解析为 `RedisType` 枚举 |
| TC-1.1.6 | 无效 Redis 类型 | `invalid_type` | 返回反序列化错误 |
| TC-1.1.7 | 过滤配置解析 | DB 黑名单、命令黑名单、Key 前缀等 | 正确解析为 Vec/HashSet |
| TC-1.1.8 | Replay 配置默认值 | 不指定 replay 字段 | 使用合理默认值（batch_size=64 等） |
| TC-1.1.9 | 常量值验证 | 访问所有常量 | 值与设计文档一致 |

#### TC-1.2 RESP 编码器测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-1.2.1 | 编码简单字符串 | `SimpleString("OK")` | `+OK\r\n` |
| TC-1.2.2 | 编码错误 | `Error("ERR unknown")` | `-ERR unknown\r\n` |
| TC-1.2.3 | 编码整数 | `Integer(42)` | `:42\r\n` |
| TC-1.2.4 | 编码负整数 | `Integer(-1)` | `:-1\r\n` |
| TC-1.2.5 | 编码批量字符串 | `BulkString("hello")` | `$5\r\nhello\r\n` |
| TC-1.2.6 | 编码空批量字符串 | `BulkString("")` | `$0\r\n\r\n` |
| TC-1.2.7 | 编码 Null | `Null` | `$-1\r\n` |
| TC-1.2.8 | 编码空数组 | `Array(vec![])` | `*0\r\n` |
| TC-1.2.9 | 编码嵌套数组 | `Array([BulkString("SET"), BulkString("key"), BulkString("val")])` | `*3\r\n$3\r\nSET\r\n$3\r\nkey\r\n$3\r\nval\r\n` |
| TC-1.2.10 | 编码嵌套 Null 数组 | `Array([Null, Integer(1)])` | `*2\r\n$-1\r\n:1\r\n` |

#### TC-1.3 RESP 解码器测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-1.3.1 | 解码简单字符串 | `+OK\r\n` | `Some(SimpleString("OK"))` |
| TC-1.3.2 | 解码错误 | `-ERR msg\r\n` | `Some(Error("ERR msg"))` |
| TC-1.3.3 | 解码整数 | `:100\r\n` | `Some(Integer(100))` |
| TC-1.3.4 | 解码批量字符串 | `$5\r\nhello\r\n` | `Some(BulkString("hello"))` |
| TC-1.3.5 | 解码 Null 批量字符串 | `$-1\r\n` | `Some(Null)` |
| TC-1.3.6 | 解码数组 | `*2\r\n$3\r\nfoo\r\n$3\r\nbar\r\n` | `Some(Array([BulkString("foo"), BulkString("bar")]))` |
| TC-1.3.7 | 解码空数组 | `*0\r\n` | `Some(Array([]))` |
| TC-1.3.8 | 解码 Null 数组 | `*-1\r\n` | `Some(Null)` |
| TC-1.3.9 | 解码嵌套数组 | `*2\r\n*2\r\n:1\r\n:2\r\n*2\r\n:3\r\n:4\r\n` | 正确的嵌套结构 |
| TC-1.3.10 | 不完整数据 | `+OK` （无 `\r\n`） | `None`（等待更多数据） |
| TC-1.3.11 | 多条消息连续 | `+OK\r\n:42\r\n` | 第一次调用返回 OK，第二次返回 42 |
| TC-1.3.12 | offset 跟踪 | 解码多条消息 | offset 正确累加 |
| TC-1.3.13 | 非法类型标记 | `?unknown\r\n` | 返回解析错误 |
| TC-1.3.14 | 非法长度 | `$-2\r\n` | 返回解析错误 |

#### TC-1.4 日志系统测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-1.4.1 | 初始化 stdout 日志 | level=info | 日志输出到 stdout |
| TC-1.4.2 | 初始化文件日志 | dir=/tmp/logs, level=debug | 日志文件正确生成 |
| TC-1.4.3 | 日志级别过滤 | level=warn | debug/info 日志被过滤 |
| TC-1.4.4 | 动态调整日志级别 | info → debug | 后续日志级别变为 debug |

#### TC-1.5 工具函数测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-1.5.1 | FNV hash 空数据 | `fnv_hash(b"")` | 返回 FNV offset basis |
| TC-1.5.2 | FNV hash 已知值 | `fnv_hash(b"hello")` | 返回已知 hash 值 |
| TC-1.5.3 | FNV hash 一致性 | 相同输入多次调用 | 返回相同结果 |

---

## 阶段二功能性测试文档

### 测试概述

| 项目 | 内容 |
|------|------|
| **测试阶段** | Phase 2 - 存储与过滤层 |
| **测试范围** | 本地文件存储、过滤系统、Checkpoint 管理 |
| **测试类型** | 单元测试 + 集成测试 |
| **前置条件** | Phase 1 全部完成 |

### 测试用例

#### TC-2.1 存储系统 - Writer 测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-2.1.1 | 创建 RDB Writer | run_id="abc", offset=0, size=1024 | 文件 `{base}/abc/0_1024.rdb` 创建成功 |
| TC-2.1.2 | RDB Writer 写入数据 | 写入 100 字节 | 文件大小为 100 字节 |
| TC-2.1.3 | RDB Writer 关闭 | 调用 close() | 文件正确刷盘 |
| TC-2.1.4 | 创建 AOF Writer | run_id="abc", offset=100 | 文件 `{base}/abc/100.aof` 创建成功 |
| TC-2.1.5 | AOF Writer 写入数据 | 写入 RESP 编码命令 | 文件内容正确 |
| TC-2.1.6 | AOF 文件轮转 | 写入超过 log_size 的数据 | 自动创建新 AOF 文件 |
| TC-2.1.7 | 重复创建 Writer | 相同 offset 创建两次 | 第二次返回错误或复用 |

#### TC-2.2 存储系统 - Reader 测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-2.2.1 | 获取 RDB Reader | offset=0（存在 RDB 文件） | 返回 RDB Reader |
| TC-2.2.2 | 获取 AOF Reader | offset=100（存在 AOF 文件） | 返回 AOF Reader |
| TC-2.2.3 | Reader 读取 RDB 数据 | 读取已写入的 RDB | 内容与写入一致 |
| TC-2.2.4 | Reader 读取 AOF 数据 | 读取已写入的 AOF | 内容与写入一致 |
| TC-2.2.5 | Reader 跨 AOF 段读取 | 多个 AOF 段 | 连续读取所有段 |
| TC-2.2.6 | 获取不存在的 Reader | 无效 offset | 返回错误 |
| TC-2.2.7 | Reader 判断类型 | RDB offset / AOF offset | 正确返回 RDB 或 AOF 类型 |

#### TC-2.3 存储系统 - DataSet 管理测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-2.3.1 | 初始化空 DataSet | 空目录 | DataSet 为空 |
| TC-2.3.2 | 初始化扫描目录 | 包含 RDB 和 AOF 文件的目录 | 正确重建内存状态 |
| TC-2.3.3 | GC 清理旧文件 | maxSize=1MB，当前 2MB | 删除旧文件直到 < 1MB |
| TC-2.3.4 | GC 保留最新文件 | GC 触发 | 最新的 RDB/AOF 不被删除 |
| TC-2.3.5 | verify_run_id | 存在的 run_id | 返回 true |
| TC-2.3.6 | verify_run_id | 不存在的 run_id | 返回 false |

#### TC-2.4 过滤系统 - Trie 树测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-2.4.1 | 插入和精确匹配 | insert("SET"), exact_match("SET") | true |
| TC-2.4.2 | 精确匹配不存在 | insert("SET"), exact_match("GET") | false |
| TC-2.4.3 | 前缀匹配 | insert("user:"), starts_with("user:123") | true |
| TC-2.4.4 | 前缀匹配失败 | insert("user:"), starts_with("order:123") | false |
| TC-2.4.5 | 空 Trie 匹配 | 空 Trie, exact_match("anything") | false |
| TC-2.4.6 | 多条插入 | insert 多个 key，分别匹配 | 各自正确匹配 |
| TC-2.4.7 | 大小写敏感 | insert("SET"), exact_match("set") | false（区分大小写） |

#### TC-2.5 过滤系统 - RangeList 测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-2.5.1 | 单范围包含 | ranges=[(0, 100)], contains(50) | true |
| TC-2.5.2 | 单范围边界 | ranges=[(0, 100)], contains(0) / contains(100) | true |
| TC-2.5.3 | 单范围不包含 | ranges=[(0, 100)], contains(101) | false |
| TC-2.5.4 | 多范围包含 | ranges=[(0,100), (200,300)], contains(250) | true |
| TC-2.5.5 | 多范围间隙 | ranges=[(0,100), (200,300)], contains(150) | false |
| TC-2.5.6 | 空 RangeList | 空 ranges, contains(any) | false |
| TC-2.5.7 | 从配置构建 | SlotRange 列表 | 正确构建 RangeList |

#### TC-2.6 过滤系统 - RedisKeyFilter 测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-2.6.1 | DB 黑名单过滤 | db_black_list=[1], db=1 | 过滤掉（返回 true） |
| TC-2.6.2 | DB 不在黑名单 | db_black_list=[1], db=0 | 不过滤（返回 false） |
| TC-2.6.3 | 命令黑名单过滤 | cmd_black_list=["FLUSHDB"], cmd="FLUSHDB" | 过滤掉 |
| TC-2.6.4 | 命令不在黑名单 | cmd_black_list=["FLUSHDB"], cmd="SET" | 不过滤 |
| TC-2.6.5 | Key 前缀白名单 | white_list=["user:"], key="user:123" | 不过滤 |
| TC-2.6.6 | Key 前缀白名单不匹配 | white_list=["user:"], key="order:123" | 过滤掉 |
| TC-2.6.7 | Key 前缀黑名单 | black_list=["temp:"], key="temp:abc" | 过滤掉 |
| TC-2.6.8 | Key 前缀黑名单不匹配 | black_list=["temp:"], key="user:abc" | 不过滤 |
| TC-2.6.9 | Slot 白名单 | white_list=[(0,100)], slot=50 | 不过滤 |
| TC-2.6.10 | Slot 白名单不匹配 | white_list=[(0,100)], slot=200 | 过滤掉 |
| TC-2.6.11 | NoRoute 命令 | cmd="AUTH" | 过滤掉（永不转发） |
| TC-2.6.12 | 多维度组合过滤 | db 在黑名单 + key 在白名单 | db 黑名单优先，过滤掉 |

#### TC-2.7 Checkpoint 管理测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-2.7.1 | 写入 Checkpoint | CheckpointInfo{master_replid, offset, ...} | 目标 Redis Hash 正确写入 |
| TC-2.7.2 | 读取 Checkpoint | 已写入的 master_replid | 返回正确的 CheckpointInfo |
| TC-2.7.3 | 读取不存在的 Checkpoint | 不存在的 master_replid | 返回 None |
| TC-2.7.4 | 更新 Checkpoint | 更新 offset | Hash 字段值更新 |
| TC-2.7.5 | Checkpoint Hash 映射 | master_replid → checkpoint_name | 映射正确写入和读取 |

---

## 阶段三功能性测试文档

### 测试概述

| 项目 | 内容 |
|------|------|
| **测试阶段** | Phase 3 - 同步引擎层 |
| **测试范围** | RDB 解析、RedisInput、RedisOutput、Syncer 状态机、端到端同步 |
| **测试类型** | 单元测试 + 集成测试 |
| **前置条件** | Phase 1、Phase 2 全部完成；需要运行中的 Redis 实例 |

### 测试环境

| 组件 | 配置 |
|------|------|
| 源 Redis | 127.0.0.1:6379，无密码 |
| 目标 Redis | 127.0.0.1:6380，无密码 |
| 本地存储目录 | /tmp/redis-ha-tool-test/ |

### 测试用例

#### TC-3.1 RDB 解析器测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-3.1.1 | 解析空 RDB | 仅包含头部和尾部校验的 RDB | 无 BinEntry 输出 |
| TC-3.1.2 | 解析 String 类型 | 包含 string key 的 RDB | 正确输出 BinEntry{type=STRING} |
| TC-3.1.3 | 解析 Hash 类型 | 包含 hash key 的 RDB | 正确输出 BinEntry{type=HASH} |
| TC-3.1.4 | 解析 List 类型 | 包含 list key 的 RDB | 正确输出 BinEntry{type=LIST} |
| TC-3.1.5 | 解析 Set 类型 | 包含 set key 的 RDB | 正确输出 BinEntry{type=SET} |
| TC-3.1.6 | 解析 ZSet 类型 | 包含 zset key 的 RDB | 正确输出 BinEntry{type=ZSET} |
| TC-3.1.7 | 解析带过期时间的 Key | 包含 TTL 的 key | BinEntry.expire 正确设置 |
| TC-3.1.8 | 解析多 DB | 包含 DB 0 和 DB 1 的 RDB | 各 BinEntry.db 正确区分 |
| TC-3.1.9 | 解析损坏的 RDB | 截断的 RDB 文件 | 返回 Corrupted 错误 |

#### TC-3.2 RedisInput 测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-3.2.1 | sync_meta 全量判断 | 源 run_id 与本地不同 | 返回 FullSync 策略 |
| TC-3.2.2 | sync_meta 增量判断 | 源 run_id 相同 + 有效 offset | 返回 IncrSync 策略 |
| TC-3.2.3 | sync_meta 本地无数据 | 本地 Channel 为空 | 返回 FullSync 策略 |
| TC-3.2.4 | PSYNC 全量响应 | 源返回 FULLRESYNC | 正确解析 run_id 和 offset |
| TC-3.2.5 | PSYNC 增量响应 | 源返回 CONTINUE | 进入增量同步模式 |
| TC-3.2.6 | RDB 数据接收 | FULLRESYNC 后的 RDB 流 | 正确写入本地 Storer |
| TC-3.2.7 | AOF 数据接收 | RDB 后的增量命令 | 正确写入 AOF Writer |
| TC-3.2.8 | REPLCONF ACK 发送 | 同步进行中 | 定期发送 ACK，offset 正确 |
| TC-3.2.9 | 连接断开重连 | 源 Redis 重启 | 自动重连并重新协商 |

#### TC-3.3 RedisOutput 测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-3.3.1 | RDB 单 key 回放 | 包含 1 个 string key 的 RDB | 目标 Redis 中该 key 存在 |
| TC-3.3.2 | RDB 多类型回放 | string/hash/list/set/zset | 所有类型数据正确写入目标 |
| TC-3.3.3 | RDB 并行回放 | 1000 个 key，parallel=4 | 所有 key 正确写入，无遗漏 |
| TC-3.3.4 | RDB 带过期时间回放 | 包含 TTL 的 key | 目标 key 的 TTL 正确设置 |
| TC-3.3.5 | AOF 单命令回放 | SET key value | 目标 Redis 中 key=value |
| TC-3.3.6 | AOF 批量回放 | 100 条 SET 命令 | 所有 key 正确写入 |
| TC-3.3.7 | AOF pipeline 模式 | pipeline=true | 使用 pipeline 批量发送 |
| TC-3.3.8 | AOF 事务模式 | transaction=true | 使用 MULTI/EXEC 包装 |
| TC-3.3.9 | Circle Key 写入 | 写命令 SET key val | 同时写入 redis_ha_tool_circle_{md5} |
| TC-3.3.10 | Circle Key 防循环 | 命令携带已存在的 circle key | 命令被丢弃 |
| TC-3.3.11 | 过滤后回放 | 配置 DB 黑名单 | 黑名单 DB 的数据不写入目标 |

#### TC-3.4 Syncer 状态机测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-3.4.1 | 初始状态 | 新建 Syncer | 状态为 ReadyRun |
| TC-3.4.2 | 状态转换 Started→FullInit | 开始全量同步 | 状态正确转换 |
| TC-3.4.3 | 状态转换 FullInit→FullSyncing | RDB 开始传输 | 状态正确转换 |
| TC-3.4.4 | 状态转换 FullSyncing→FullSynced | RDB 传输完成 | 状态正确转换 |
| TC-3.4.5 | 状态转换 FullSynced→IncrSyncing | 开始增量同步 | 状态正确转换 |
| TC-3.4.6 | 状态通知 | 状态变化 | watch receiver 收到通知 |
| TC-3.4.7 | Pause 暂停 | 调用 pause() | 状态变为 Pause，同步暂停 |
| TC-3.4.8 | Resume 恢复 | 调用 resume() | 状态恢复为 Run，同步继续 |
| TC-3.4.9 | Stop 停止 | 调用 stop() | 状态变为 Stop，所有 Task 退出 |

#### TC-3.5 事务跟踪测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-3.5.1 | 非事务命令 | SET key val | 状态保持 No |
| TC-3.5.2 | MULTI 命令 | MULTI | 状态变为 Begin |
| TC-3.5.3 | 事务内命令 | SET key1 val1 (在 MULTI 后) | 状态变为 In |
| TC-3.5.4 | EXEC 命令 | EXEC | 状态变为 Commit，然后回到 No |
| TC-3.5.5 | DISCARD 命令 | DISCARD | 状态回到 No |

#### TC-3.6 端到端集成测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-3.6.1 | 全量同步 - String | 源端写入 100 个 string key | 目标端 100 个 key 值一致 |
| TC-3.6.2 | 全量同步 - Hash | 源端写入 hash 类型 | 目标端 hash 字段和值一致 |
| TC-3.6.3 | 全量同步 - List | 源端写入 list 类型 | 目标端 list 元素和顺序一致 |
| TC-3.6.4 | 全量同步 - Set | 源端写入 set 类型 | 目标端 set 成员一致 |
| TC-3.6.5 | 全量同步 - ZSet | 源端写入 zset 类型 | 目标端 zset 成员和分数一致 |
| TC-3.6.6 | 全量同步 - 带 TTL | 源端写入带过期时间的 key | 目标端 key 有相同 TTL |
| TC-3.6.7 | 全量同步 - 多 DB | 源端 DB 0 和 DB 1 都有数据 | 目标端对应 DB 数据一致 |
| TC-3.6.8 | 增量同步 - SET | 全量完成后，源端 SET 新 key | 目标端实时出现该 key |
| TC-3.6.9 | 增量同步 - DEL | 源端 DEL 一个 key | 目标端该 key 被删除 |
| TC-3.6.10 | 增量同步 - HSET | 源端 HSET 操作 | 目标端 hash 正确更新 |
| TC-3.6.11 | 增量同步 - LPUSH | 源端 LPUSH 操作 | 目标端 list 正确更新 |
| TC-3.6.12 | 增量同步 - 事务 | 源端 MULTI/EXEC | 目标端事务正确执行 |
| TC-3.6.13 | 断点续传 | 同步中停止，重启 | 从 checkpoint 恢复，数据不丢失 |
| TC-3.6.14 | 过滤 - DB 黑名单 | 配置 db_black_list=[1] | DB 1 的数据不同步 |
| TC-3.6.15 | 过滤 - Key 前缀 | 配置 key_prefix_white_list=["user:"] | 仅 user: 开头的 key 同步 |
| TC-3.6.16 | 过滤 - 命令黑名单 | 配置 cmd_black_list=["DEL"] | DEL 命令不同步 |
| TC-3.6.17 | 大数据量同步 | 源端 10 万 key | 全部正确同步，无遗漏 |

---

## 阶段四功能性测试文档

### 测试概述

| 项目 | 内容 |
|------|------|
| **测试阶段** | Phase 4 - 高可用与运维层 |
| **测试范围** | 集群选举、gRPC 副本、HTTP API、Prometheus 监控、集群模式 |
| **测试类型** | 单元测试 + 集成测试 + 端到端测试 |
| **前置条件** | Phase 1-3 全部完成；需要 Redis、可选 etcd 环境 |

### 测试环境

| 组件 | 配置 |
|------|------|
| 源 Redis | 127.0.0.1:6379 |
| 目标 Redis | 127.0.0.1:6380 |
| Syncer 实例 A | 127.0.0.1:8080 (HTTP) + 9090 (gRPC) |
| Syncer 实例 B | 127.0.0.1:8081 (HTTP) + 9091 (gRPC) |
| etcd（可选） | 127.0.0.1:2379 |

### 测试用例

#### TC-4.1 集群选举 - Redis 后端测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-4.1.1 | 首次竞选 | 单节点调用 campaign() | 成为 Leader |
| TC-4.1.2 | 查询 Leader | 调用 leader() | 返回当前 Leader 标识 |
| TC-4.1.3 | 续期 | Leader 调用 renew() | 租约续期成功 |
| TC-4.1.4 | 退选 | Leader 调用 resign() | 不再是 Leader |
| TC-4.1.5 | 竞争竞选 | 两个节点同时 campaign() | 仅一个成为 Leader |
| TC-4.1.6 | Leader 故障重选 | kill Leader，等待租约过期 | 另一节点成为新 Leader |
| TC-4.1.7 | 注册与发现 | register(key, value) + discover(key) | 返回注册的节点列表 |

#### TC-4.2 gRPC 副本同步测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-4.2.1 | Follower 连接 Leader | Follower 发起 Sync RPC | 连接建立，收到 META 消息 |
| TC-4.2.2 | 数据流传输 | Leader 写入 AOF 数据 | Follower 收到数据并写入本地 |
| TC-4.2.3 | 增量数据同步 | Leader 持续接收源端命令 | Follower 实时收到增量数据 |
| TC-4.2.4 | Follower 断线重连 | Follower 网络断开后恢复 | 自动重连，从断点继续 |
| TC-4.2.5 | Leader 故障转移 | kill Leader | Follower 竞选为新 Leader |
| TC-4.2.6 | 手动 handover | 调用 handover API | Leader/Follower 角色互换 |
| TC-4.2.7 | 多 Follower | 2 个 Follower 连接 1 个 Leader | 两个 Follower 都收到数据 |

#### TC-4.3 HTTP API 测试

| 用例 ID | 测试项 | 方法 | 路径 | 预期结果 |
|---------|--------|------|------|----------|
| TC-4.3.1 | 健康检查 | GET | `/health` | 200，返回 `{"status":"ok"}` |
| TC-4.3.2 | 查询状态 | GET | `/syncer/status` | 200，返回当前状态（Running/Paused/...） |
| TC-4.3.3 | 查询配置 | GET | `/syncer/config` | 200，返回当前配置（脱敏） |
| TC-4.3.4 | 停止同步 | POST | `/syncer/stop` | 200，同步状态变为 Stopped |
| TC-4.3.5 | 暂停同步 | POST | `/syncer/pause` | 200，同步状态变为 Paused |
| TC-4.3.6 | 恢复同步 | POST | `/syncer/resume` | 200，同步状态恢复为 Running |
| TC-4.3.7 | 重启同步 | POST | `/syncer/restart` | 200，同步器重启 |
| TC-4.3.8 | 强制全量同步 | POST | `/syncer/fullsync` | 200，触发全量同步 |
| TC-4.3.9 | 手动切换 Leader | POST | `/syncer/handover` | 200，角色切换 |
| TC-4.3.10 | 存储 GC | POST | `/storage/gc` | 200，旧文件被清理 |
| TC-4.3.11 | 调整日志级别 | PUT | `/log_level` | 200，日志级别更新 |
| TC-4.3.12 | Dump 任务栈 | GET | `/dumpstack` | 200，返回当前任务栈信息 |
| TC-4.3.13 | Prometheus 指标 | GET | `/metrics` | 200，返回 Prometheus 格式文本 |
| TC-4.3.14 | 暂停后恢复数据一致性 | POST pause → 源端写入 → POST resume | 恢复后数据最终一致 |
| TC-4.3.15 | 停止后状态验证 | POST stop → GET status | 状态为 Stopped |

#### TC-4.4 Prometheus 监控测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-4.4.1 | 同步延迟指标 | 同步运行中 | `sync_delay_seconds` 值 > 0 |
| TC-4.4.2 | RDB 回放进度 | 全量同步中 | `rdb_replay_progress` 从 0 到 100 |
| TC-4.4.3 | AOF 命令计数 | 增量同步中 | `aof_commands_total` 持续增加 |
| TC-4.4.4 | 过滤命令计数 | 有命令被过滤 | `filtered_commands_total` > 0 |
| TC-4.4.5 | 错误计数 | 产生错误时 | `sync_errors_total` 增加 |
| TC-4.4.6 | 指标格式正确 | GET /metrics | 符合 Prometheus text 格式 |

#### TC-4.5 集群模式测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-4.5.1 | Sentinel 模式连接 | 配置 sentinel 类型 | 自动发现 master 并连接 |
| TC-4.5.2 | Sentinel failover | Sentinel 触发 failover | Syncer 自动连接到新 master |
| TC-4.5.3 | Cluster 模式连接 | 配置 cluster 类型 | 连接到所有 shard master |
| TC-4.5.4 | Cluster 多 shard 同步 | 3 shard 集群 | 所有 shard 数据正确同步 |
| TC-4.5.5 | Cluster 拓扑变化 | 添加/移除 shard | 检测到变化并重新连接 |
| TC-4.5.6 | Cluster slot 迁移 | slot 从一个 node 迁移到另一个 | 同步正确跟随迁移 |

#### TC-4.6 端到端高可用测试

| 用例 ID | 测试项 | 输入 | 预期结果 |
|---------|--------|------|----------|
| TC-4.6.1 | 双节点 HA 部署 | 启动 Syncer A + B | 一个 Leader，一个 Follower |
| TC-4.6.2 | Leader 同步数据 | 源端写入数据 | 目标端数据一致 |
| TC-4.6.3 | Leader 故障转移 | kill Leader 进程 | Follower 自动成为 Leader |
| TC-4.6.4 | 故障转移后数据连续 | 故障转移期间源端持续写入 | 数据无丢失（可能有短暂延迟） |
| TC-4.6.5 | 原 Leader 恢复 | 重启原 Leader | 作为 Follower 加入 |
| TC-4.6.6 | 长时间运行稳定性 | 运行 24 小时 | 无内存泄漏、无崩溃 |

---

## 附录：命名规范

### 文件命名

- 文件名使用 `snake_case`（如 `key_filter.rs`、`state_machine.rs`）
- 测试文件使用 `test_` 前缀或放在 `tests/` 目录

### 代码命名

| 类型 | 规范 | 示例 |
|------|------|------|
| 模块名 | `snake_case` | `filter::key_filter` |
| struct | `PascalCase` | `RedisKeyFilter`, `SyncConfig` |
| enum | `PascalCase` | `SyncState`, `RespValue` |
| enum variant | `PascalCase` | `SyncState::ReadyRun` |
| trait | `PascalCase` | `Syncer`, `Input`, `Output` |
| 函数/方法 | `snake_case` | `sync_meta()`, `send_rdb()` |
| 变量 | `snake_case` | `run_id`, `batch_size` |
| 常量 | `SCREAMING_SNAKE_CASE` | `CHECKPOINT_KEY`, `CIRCLE_PREFIX_KEY` |
| 类型参数 | 单大写字母或 `PascalCase` | `T`, `E` |
| 生命周期 | 小写字母加 `'` | `'a`, `'ctx` |

### 注释规范

```rust
/// 模块级文档注释（中文）
/// 
/// 本模块实现了 RESP 协议的编解码功能。

/// 结构体/枚举文档注释
/// 
/// 描述用途和关键设计决策。
pub struct RespDecoder {
    /// 内部缓冲区，用于累积待解码的字节数据
    buf: BytesMut,
    /// 当前复制偏移量，每解码一个字节递增
    offset: u64,
}

impl RespDecoder {
    /// 尝试从缓冲区解码一个 RESP 值
    /// 
    /// 如果缓冲区数据不完整，返回 `None`，等待更多数据。
    /// 如果数据格式错误，返回 `Err`。
    pub fn decode(&mut self) -> Result<Option<RespValue>> {
        // 单行注释：解释关键逻辑步骤
        ...
    }
}

// 行内注释：解释不直观的代码逻辑
let idx = fnv_hash(&entry.key) % workers.len(); // 按 key hash 分发到 worker
```

### 单元测试规范

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用例：解码简单字符串
    /// 
    /// 验证 `+OK\r\n` 能正确解码为 `SimpleString("OK")`。
    #[test]
    fn test_decode_simple_string() {
        let mut decoder = RespDecoder::new();
        decoder.feed(b"+OK\r\n");
        let result = decoder.decode().unwrap();
        assert_eq!(result, Some(RespValue::SimpleString(Bytes::from("OK"))));
    }
}
```
