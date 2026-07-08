# Redis Tool 安装、部署和使用手册

## 2. 部署指南

### 2.1 部署架构

#### 2.1.1 单节点部署

最简单的部署方式，适合测试和小规模生产环境。

```
┌─────────────────┐
│   Redis Tool    │
│  (单节点)        │
└────────┬────────┘
         │
    ┌────┴────┐
    │         │
┌───▼───┐ ┌──▼────┐
│源Redis│ │目标Redis│
│(6379) │ │(6380)  │
└───────┘ └────────┘
```

#### 2.1.2 高可用部署

使用 Leader-Follower 模式，自动故障转移。

```
┌──────────────┐   ┌──────────────┐
│ Redis Tool   │   │ Redis Tool   │
│  (Leader)    │◄──│ (Follower)   │
└──────┬───────┘   └──────┬───────┘
       │                  │
       │                  │
   ┌───▼───────────────▼───┐
   │   Redis / etcd       │
   │   (选举后端)         │
   └──────────────────────┘
```

### 2.2 配置文件详解

#### 2.2.1 配置文件位置

- 默认路径：`config/config.json`
- 或通过 `--config` 参数指定

#### 2.2.2 配置文件示例

```json
{
  "license": {
    "key": "your-license-key"
  },
  "server": {
    "httpPort": 8080,
    "grpcPort": 9090
  },
  "cluster": {
    "enabled": false,
    "nodeId": "node-1",
    "electionBackend": "redis"
  },
  "input": {
    "addresses": ["127.0.0.1:6379"],
    "password": "source-password",
    "authType": "password",
    "redisType": "standalone",
    "replay": {
      "resumeFromBreakPoint": true,
      "keyExists": "replace",
      "rdbParallel": 4,
      "pipeline": false,
      "batchSize": 64,
      "batchCount": 100
    },
    "filter": {
      "dbBlackList": [],
      "cmdBlackList": ["FLUSHDB", "FLUSHALL"],
      "keyPrefixWhiteList": [],
      "keyPrefixBlackList": []
    }
  },
  "localCache": {
    "dir": "/var/lib/redis-ha-tool",
    "maxSize": 10737418240,
    "logSize": 52428800,
    "headerSize": 4096
  },
  "output": {
    "addresses": ["127.0.0.1:6380"],
    "password": "target-password",
    "authType": "password",
    "redisType": "standalone"
  },
  "log": {
    "level": "info",
    "dir": "/var/log/redis-ha-tool",
    "stdout": false,
    "maxAge": 7,
    "maxFiles": 10,
    "maxSize": 104857600
  }
}
```

#### 2.2.3 关键配置说明

**源 Redis 配置（input）**：

| 字段 | 说明 | 示例值 |
|------|------|--------|
| `addresses` | Redis 地址列表 | `["127.0.0.1:6379"]` |
| `password` | Redis 密码 | `"password"` |
| `authType` | 认证类型 | `"password"` / `"acl"` / `"none"` |
| `redisType` | Redis 类型 | `"standalone"` / `"sentinel"` / `"cluster"` |

**目标 Redis 配置（output）**：

同 `input` 配置。

**同步配置（replay）**：

| 字段 | 说明 | 默认值 |
|------|------|--------|
| `resumeFromBreakPoint` | 是否断点续传 | `true` |
| `keyExists` | Key 存在策略 | `"replace"` |
| `rdbParallel` | RDB 并行数 | `4` |
| `pipeline` | 是否 pipeline | `false` |
| `batchSize` | 批量大小 | `64` |
| `batchCount` | 批量数量 | `100` |

**过滤配置（filter）**：

| 字段 | 说明 | 示例 |
|------|------|------|
| `dbBlackList` | DB 黑名单 | `[1, 2]` |
| `cmdBlackList` | 命令黑名单 | `["FLUSHDB"]` |
| `keyPrefixWhiteList` | Key 前缀白名单 | `["user:"]` |
| `keyPrefixBlackList` | Key 前缀黑名单 | `["temp:"]` |

**日志配置（log）**：

| 字段 | 说明 | 默认值 |
|------|------|--------|
| `level` | 日志级别 | `"info"` |
| `dir` | 日志目录 | `/var/log/redis-ha-tool` |
| `stdout` | 是否输出到终端 | `false` |


#### 2.3.3 测试运行

```bash
# 前台运行（测试）
redis-ha-tool --config /etc/redis-ha-tool/config.json

# 查看日志
tail -f /var/log/redis-ha-tool/redis-ha-tool.log

# 检查 HTTP API
curl http://localhost:8080/health

# 检查同步状态
curl http://localhost:8080/syncer/status
```

### 2.4 systemd 服务配置

创建 systemd 服务文件：

```bash
sudo vi /etc/systemd/system/redis-ha-tool.service
```

内容：

```ini
[Unit]
Description=Redis Syncer - High-performance Redis data synchronization tool
Documentation=https://github.com/your-org/redis-tool
After=network.target redis.service

[Service]
Type=simple
User=redis-ha-tool
Group=redis-ha-tool
WorkingDirectory=/var/lib/redis-ha-tool
ExecStart=/usr/local/bin/redis-ha-tool --config /etc/redis-ha-tool/config.json
ExecStop=/bin/kill -SIGTERM $MAINPID
Restart=on-failure
RestartSec=5s
LimitNOFILE=65536

# 环境变量
Environment=RUST_LOG=info

# 日志
StandardOutput=journal
StandardError=journal

# 安全限制
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

**启用服务**：

```bash
# 加载 systemd 配置
sudo systemctl daemon-reload

# 启动服务
sudo systemctl start redis-ha-tool

# 设置开机启动
sudo systemctl enable redis-ha-tool

# 查看状态
sudo systemctl status redis-ha-tool

# 查看日志
sudo journalctl -u redis-ha-tool -f
```

### 2.5 高可用部署（可选）

#### 2.5.1 准备选举后端

**使用 Redis 作为选举后端**：

```bash
# 确保 Redis 可用
redis-cli ping
# 输出：PONG
```

**使用 etcd 作为选举后端**：

```bash
# 安装 etcd（如未安装）
# 参考：https://etcd.io/docs/latest/install/

# 启动 etcd
etcd --listen-client-urls http://0.0.0.0:2379 \
     --advertise-client-urls http://localhost:2379
```

#### 2.5.2 配置集群模式

编辑 `/etc/redis-ha-tool/config.json`：

```json
{
  "cluster": {
    "enabled": true,
    "nodeId": "node-1",
    "electionBackend": "redis"
  }
}
```

#### 2.5.3 启动多个节点

**节点 1**：

```bash
# 配置 nodeId = "node-1"
redis-ha-tool --config /etc/redis-ha-tool/node1-config.json
```

**节点 2**：

```bash
# 配置 nodeId = "node-2"
redis-ha-tool --config /etc/redis-ha-tool/node2-config.json
```

#### 2.5.4 验证高可用

```bash
# 查看当前 Leader
curl http://localhost:8080/syncer/status

# 查看选举状态
redis-cli GET redis_ha_tool_input_election_sync_leader

# 模拟 Leader 故障
sudo systemctl stop redis-ha-tool@node1

# 验证 Follower 自动接管
curl http://localhost:8081/syncer/status
```

---

## 3. 使用手册

### 3.1 启动和停止

#### 3.1.1 启动同步器

```bash
# 方式 1：systemd 服务
sudo systemctl start redis-ha-tool

# 方式 2：直接运行
redis-ha-tool --config /etc/redis-ha-tool/config.json

# 方式 3：Docker
docker start redis-ha-tool
```

#### 3.1.2 停止同步器

```bash
# 方式 1：systemd 服务
sudo systemctl stop redis-ha-tool

# 方式 2：HTTP API（优雅停止）
curl -X POST http://localhost:8080/syncer/stop

# 方式 3：Docker
docker stop redis-ha-tool
```

#### 3.1.3 重启同步器

```bash
# systemd
sudo systemctl restart redis-ha-tool

# HTTP API
curl -X POST http://localhost:8080/syncer/restart
```

### 3.2 HTTP 管理 API

#### 3.2.1 API 端点列表

| 端点 | 方法 | 功能 |
|------|------|------|
| `/health` | GET | 健康检查 |
| `/syncer/status` | GET | 查询同步状态 |
| `/syncer/stop` | POST | 停止同步 |
| `/syncer/pause` | POST | 暂停同步 |
| `/syncer/resume` | POST | 恢复同步 |
| `/syncer/restart` | POST | 重启同步 |
| `/metrics` | GET | Prometheus 指标 |

#### 3.2.2 健康检查

```bash
curl http://localhost:8080/health

# 返回：
# {
#   "code": 200,
#   "message": "success",
#   "data": {
#     "status": "ok"
#   }
# }
```

#### 3.2.3 查询状态

```bash
curl http://localhost:8080/syncer/status

# 返回：
# {
#   "code": 200,
#   "message": "success",
#   "data": {
#     "state": "Run",
#     "role": "Leader"
#   }
# }
```

**状态说明**：

- `state`：
  - `ReadyRun`：准备运行
  - `Run`：正在运行
  - `Pause`：已暂停
  - `Stop`：已停止

- `role`：
  - `Leader`：主同步器
  - `Follower`：从同步器

#### 3.2.4 控制同步

**暂停同步**：

```bash
curl -X POST http://localhost:8080/syncer/pause

# 返回：
# {
#   "code": 200,
#   "message": "success",
#   "data": {
#     "result": "paused"
#   }
# }
```

**恢复同步**：

```bash
curl -X POST http://localhost:8080/syncer/resume

# 返回：
# {
#   "code": 200,
#   "message": "success",
#   "data": {
#     "result": "resumed"
#   }
# }
```

**停止同步**：

```bash
curl -X POST http://localhost:8080/syncer/stop

# 返回：
# {
#   "code": 200,
#   "message": "success",
#   "data": {
#     "result": "stopped"
#   }
# }
```

### 3.3 同步状态监控

#### 3.3.1 查看同步进度

```bash
# 查看 RDB 回放进度
curl http://localhost:8080/metrics | grep rdb_replay_progress

# 查看 AOF 命令计数
curl http://localhost:8080/metrics | grep aof_commands_total

# 查看同步延迟
curl http://localhost:8080/metrics | grep sync_delay_seconds
```

#### 3.3.2 Prometheus 监控

```bash
# 配置 Prometheus scrape
# prometheus.yml
scrape_configs:
  - job_name: 'redis-ha-tool'
    static_configs:
      - targets: ['localhost:8080']

# 查询示例：
# - 同步延迟：sync_delay_seconds
# - RDB 进度：rdb_replay_progress
# - AOF 命令：aof_commands_total
# - 过滤计数：filtered_commands_total
# - 错误计数：sync_errors_total
```

### 3.4 数据过滤使用

#### 3.4.1 配置过滤规则

编辑配置文件：

```json
{
  "input": {
    "filter": {
      "dbBlackList": [1, 2],
      "cmdBlackList": ["FLUSHDB", "FLUSHALL"],
      "keyPrefixWhiteList": ["user:", "order:"],
      "keyPrefixBlackList": ["temp:", "cache:"]
    }
  }
}
```

#### 3.4.2 过滤规则说明

**DB 黑名单**：

- 跳过指定 DB 的所有数据
- 示例：`[1, 2]` 表示 DB 1 和 DB 2 的数据不同步

**命令黑名单**：

- 不转发指定命令
- 示例：`["FLUSHDB"]` 表示 FLUSHDB 命令不转发

**Key 前缀白名单**：

- 仅同步指定前缀的 Key
- 示例：`["user:"]` 表示仅同步 `user:` 开头的 Key

**Key 前缀黑名单**：

- 不同步指定前缀的 Key
- 示例：`["temp:"]` 表示 `temp:` 开头的 Key 不同步

#### 3.4.3 验证过滤效果

```bash
# 查看过滤统计
curl http://localhost:8080/metrics | grep filtered_commands_total

# 输出：
# filtered_commands_total 1234
```

### 3.5 断点续传使用

#### 3.5.1 启用断点续传

配置文件：

```json
{
  "input": {
    "replay": {
      "resumeFromBreakPoint": true
    }
  }
}
```

#### 3.5.2 Checkpoint 查询

```bash
# 查询目标 Redis 的 checkpoint
redis-cli HGETALL redis_ha_tool_checkpoint

# 输出（使用 master_replid 实际值作为字段前缀）：
# 1) "5e2f1b3a2c4d6e8f0a1b2c3d4e5f6a7b8c9d0e1f"
# 2) "12345"
# 3) "5e2f1b3a2c4d6e8f0a1b2c3d4e5f6a7b8c9d0e1f_offset"
# 4) "12345"
# 5) "5e2f1b3a2c4d6e8f0a1b2c3d4e5f6a7b8c9d0e1f_version"
# 6) "5"
```

#### 3.5.3 手动恢复

```bash
# 停止同步
curl -X POST http://localhost:8080/syncer/stop

# 重启同步（自动从 checkpoint 恢复）
curl -X POST http://localhost:8080/syncer/restart
```

---

## 4. 运维手册

### 4.1 日志管理

#### 4.1.1 日志位置

- 默认路径：`/var/log/redis-ha-tool/redis-ha-tool.log`
- Docker：`~/redis-ha-tool/logs/`

#### 4.1.2 日志级别调整

**临时调整（HTTP API）**：

```bash
# 调整为 debug
curl -X PUT http://localhost:8080/log_level -d '{"level":"debug"}'

# 调整为 warn
curl -X PUT http://localhost:8080/log_level -d '{"level":"warn"}'
```

**永久调整（配置文件）**：

```json
{
  "log": {
    "level": "debug"
  }
}
```

#### 4.1.3 日志轮转

系统自动轮转日志文件：

- 每天轮转
- 最大保留 7 天
- 单文件最大 100MB

手动清理：

```bash
# 查看日志大小
du -sh /var/log/redis-ha-tool/

# 清理旧日志
find /var/log/redis-ha-tool/ -name "*.log.*" -mtime +7 -delete
```

### 4.2 数据管理

#### 4.2.1 本地存储位置

- RDB 文件：`/var/lib/redis-ha-tool/{runId}/`
- AOF 文件：`/var/lib/redis-ha-tool/{runId}/`

#### 4.2.2 存储大小控制

配置文件：

```json
{
  "localCache": {
    "maxSize": 10737418240,  // 10GB
    "logSize": 52428800      // 50MB
  }
}
```

#### 4.2.3 手动清理存储

```bash
# HTTP API 清理
curl -X POST http://localhost:8080/storage/gc

# 手动删除
rm -rf /var/lib/redis-ha-tool/{oldRunId}/
```

### 4.3 性能调优

#### 4.3.1 RDB 并行回放

```json
{
  "input": {
    "replay": {
      "rdbParallel": 8  // 根据 CPU 核数调整
    }
  }
}
```

#### 4.3.2 AOF 批处理

```json
{
  "input": {
    "replay": {
      "pipeline": true,
      "batchSize": 128,
      "batchCount": 200
    }
  }
}
```

#### 4.3.3 连接数优化

```bash
# 查看连接数
netstat -an | grep 6379 | wc -l

# 调整系统限制
sudo sysctl -w net.core.somaxconn=65535
sudo sysctl -w net.ipv4.tcp_max_syn_backlog=65535
```

### 4.4 安全加固

#### 4.4.1 TLS 配置

```json
{
  "input": {
    "tls": {
      "enabled": true,
      "caCert": "/path/to/ca.crt",
      "clientCert": "/path/to/client.crt",
      "clientKey": "/path/to/client.key"
    }
  }
}
```

#### 4.4.2 访问控制

```bash
# 使用防火墙限制 HTTP API 访问
sudo iptables -A INPUT -p tcp --dport 8080 -s 10.0.0.0/8 -j ACCEPT
sudo iptables -A INPUT -p tcp --dport 8080 -j DROP
```

### 4.5 备份和恢复

#### 4.5.1 配置文件备份

```bash
# 备份配置
sudo cp /etc/redis-ha-tool/config.json /etc/redis-ha-tool/config.json.bak

# 定期备份
0 2 * * * cp /etc/redis-ha-tool/config.json /backup/config.json.$(date +\%Y\%m\%d)
```

#### 4.5.2 Checkpoint 备份

```bash
# 导出 checkpoint
redis-cli HGETALL redis_ha_tool_checkpoint > checkpoint_backup.txt

# 导入 checkpoint（恢复）
while read key value; do
  redis-cli HSET redis_ha_tool_checkpoint "$key" "$value"
done < checkpoint_backup.txt
```

---

## 5. 故障排查

### 5.1 常见问题

#### 5.1.1 同步延迟过高

**现象**：

```bash
curl http://localhost:8080/metrics | grep sync_delay_seconds
# sync_delay_seconds 300  # 5 分钟延迟
```

**排查步骤**：

```bash
# 1. 检查网络
ping source-redis-host
traceroute source-redis-host

# 2. 检查 Redis 负载
redis-cli -h source-redis info stats

# 3. 检查本地资源
top
df -h

# 4. 查看日志
tail -f /var/log/redis-ha-tool/redis-ha-tool.log | grep "delay"
```

**解决方案**：

- 增加并行度：`rdbParallel: 8`
- 使用 pipeline：`pipeline: true`
- 优化网络带宽

#### 5.1.2 同步停止

**现象**：

```bash
curl http://localhost:8080/syncer/status
# {"state": "Stop"}
```

**排查步骤**：

```bash
# 1. 查看日志
tail -f /var/log/redis-ha-tool/redis-ha-tool.log | grep "error"

# 2. 检查 Redis 连接
redis-cli -h source-redis ping
redis-cli -h target-redis ping

# 3. 检查 checkpoint
redis-cli HGETALL redis_ha_tool_checkpoint
```

**解决方案**：

```bash
# 重启同步
curl -X POST http://localhost:8080/syncer/restart
```

#### 5.1.3 内存占用过高

**现象**：

```bash
top
# PID 1234 redis-ha-tool 2GB
```

**排查步骤**：

```bash
# 1. 检查存储大小
du -sh /var/lib/redis-ha-tool/

# 2. 检查配置
cat /etc/redis-ha-tool/config.json | grep maxSize

# 3. 清理存储
curl -X POST http://localhost:8080/storage/gc
```

**解决方案**：

- 减小 `maxSize`
- 定期执行 GC
- 监控内存使用

### 5.2 错误代码参考

| 错误码 | 说明 | 解决方案 |
|--------|------|----------|
| `500` | 内部错误 | 查看日志，重启服务 |
| `400` | 参数错误 | 检查请求参数 |
| `404` | 资源不存在 | 检查路径 |
| `503` | 服务不可用 | 检查服务状态 |

### 5.3 日志分析

#### 5.3.1 错误日志示例

```json
{
  "timestamp": "2026-07-06T14:52:30Z",
  "level": "ERROR",
  "target": "redis_syncer::syncer::input",
  "message": "连接源 Redis 失败",
  "error": "Connection refused"
}
```

#### 5.3.2 日志查询工具

```bash
# 查看最近错误
grep "ERROR" /var/log/redis-ha-tool/redis-ha-tool.log | tail -20

# 查看特定时间段日志
awk '/2026-07-06T14:00/,/2026-07-06T15:00/' log.json

# 使用 jq 解析 JSON 日志
cat log.json | jq 'select(.level=="ERROR")'
```

### 5.4 故障恢复流程

```bash
# 1. 停止服务
sudo systemctl stop redis-ha-tool

# 2. 检查状态
redis-cli ping
curl http://localhost:8080/health

# 3. 清理临时数据
rm -rf /var/lib/redis-ha-tool/*.tmp

# 4. 重启服务
sudo systemctl start redis-ha-tool

# 5. 监控恢复
curl http://localhost:8080/syncer/status
curl http://localhost:8080/metrics
```

---

## 附录：快速命令参考

### 常用命令

```bash
# 安装
sudo systemctl start redis-ha-tool

# 状态查询
curl http://localhost:8080/syncer/status

# 停止
curl -X POST http://localhost:8080/syncer/stop

# 重启
sudo systemctl restart redis-ha-tool

# 查看日志
tail -f /var/log/redis-ha-tool/redis-ha-tool.log

# 监控
curl http://localhost:8080/metrics

# 健康检查
curl http://localhost:8080/health
```

### 配置文件模板

```bash
# 查看配置模板
cat /etc/redis-ha-tool/config.json.example

# 复制配置模板
cp /etc/redis-ha-tool/config.json.example /etc/redis-ha-tool/config.json
```
