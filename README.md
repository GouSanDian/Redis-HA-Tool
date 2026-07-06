# Redis-HA-Tool
一站式企业级 Redis 数据治理工具，面向缓存集群迁移、异地灾备、双活多活、数据备份恢复全场景，基于 Redis Replication 协议实现全量+增量数据流转，内置高可用故障切换与断点续传能力，解决传统同步工具无HA、中断重跑、仅单向同步、备份恢复割裂等痛点。

## 一、项目简介
`Redis-HA-Tool` 是面向生产环境打造的一体化 Redis 数据运维套件，统一覆盖**离线备份、数据恢复、跨集群单向复制、双向双活同步**四大核心能力。
兼容 Standalone、Sentinel、Redis Cluster 多架构，适用于机房迁移、异地容灾、同城多活、故障回滚、数据冷热归档等业务场景，具备企业级稳定性与链路自愈能力。

### 核心关键词
Redis同步、Redis双活、跨集群迁移、断点续传、主备高可用、故障自动切换、RDB备份、数据恢复、单向复制、双向同步、增量同步、全量同步、异地多活、缓存灾备、Replication协议、链路自愈

## 二、核心能力一览
### 1. 数据备份 & 恢复（离线数据治理）
- **Dump 全量备份**：解析源库 RDB/AOF，导出标准化备份文件，支持定时自动备份、分片集群并行导出
- **Restore 数据回滚**：本地备份文件一键恢复至任意 Redis 实例/集群，支持分库、Key 前缀过滤导入
- 适配数据归档、故障应急回滚、测试环境快速造数场景

### 2. 单向数据同步（迁移/灾备）
模拟 Redis Slave 拉取 Replication 数据流，全量初始化+实时增量同步：
- 支持单机 ↔ 哨兵 ↔ Cluster 异构集群互通
- 业务不停服迁移、异地只读灾备、数据分流镜像场景专用

### 3. 双向数据同步（同城/异地多活）
内置循环写入防冲突机制，支持双集群实时双向数据流转：
- 双写多活架构、同城双机房高可用、异地单元化业务
- 自动识别同源同步指令，避免环形复制数据风暴

### 4. HA 高可用架构（工具自身高可用）
- 同步服务主备双节点部署，故障自动 Failover
- 主节点宕机后备节点自动接管同步链路，不中断数据流转
- 任务元数据持久化，切换后无缝承接增量同步

### 5. 断点续传（链路自愈核心特性）
- 持久化同步位点（repl offset / AOF 偏移）
- 网络抖动、进程重启、节点宕机后，从上次中断位置增量续传，无需重跑全量
- 支持长断连场景，自动追平积压增量数据，大幅降低重同步耗时

## 三、产品核心特性

✅ **全链路一体化**：备份、恢复、单向同步、双向双活同一工具闭环，无需多组件组合

✅ **原生断点续传**：持久化同步位点，中断免全量重传，大幅节省大集群迁移成本

✅ **工具层 HA 主备切换**：同步服务集群化部署，故障自动切换，保障7×24小时同步稳定

✅ **双模式复制支持**
  - 单向同步：集群迁移、异地只读灾备、数据镜像
  - 双向同步：同城多活、异地双机房单元化业务

✅ **全架构兼容**：Standalone / Sentinel / Redis Cluster 互通同步

✅ **低侵入同步**：基于 Redis 原生 Replication 协议，源库无需改造、无业务侵入

✅ **全量+增量双通道**：首次全量初始化，后续实时增量追数，支持不停机迁移

✅ **数据一致性保障**：双向同步内置防循环复制策略，避免数据冲突、重复写入

✅ **轻量化部署**：单二进制包运行，支持 Docker、K8s 容器化运维

## 四、典型适用场景
1. **Redis 集群迁移升级**：旧集群 → 新集群单向不停服迁移
2. **异地容灾备份**：生产集群单向同步至异地备用集群
3. **同城/异地双活多活**：双机房 Redis 双向实时同步，业务双写高可用
4. **定时数据归档**：自动 RDB 备份，故障快速 Restore 回滚
5. **多单元业务数据互通**：多地域业务集群双向数据互通

## 五、对比同类工具优势
- 区别 RedisShake / redis-migrate-tool：内置**HA主备故障切换**、原生支持**稳定双向同步**，统一集成备份恢复能力
- 传统工具短板：无高可用、中断需重全量、双向同步需复杂二次开发、备份同步分离；本工具一站式解决

## 六、商业使用与联系方式
联系获得商业版许可证，或者源代码包。

<img width="190.0" height="259" alt="b6add25034c22a813d7b45b17c95762a" src="https://github.com/user-attachments/assets/cd383278-8c86-4ab5-85b6-7180a886482e" />



---
# Redis-HA-Tool
All-in-one enterprise Redis data governance toolkit, integrates Redis backup, restore, one-way replication & bidirectional active-active sync.

### Key Capabilities
- Breakpoint resume for sync tasks, no full resync after interruption
- HA master-slave failover architecture for sync service high availability
- Full RDB/AOF backup & point-in-time data restore
- Cross-cluster one-way sync for migration & disaster recovery
- Bidirectional active-active sync with circular write conflict prevention
- Compatible with Standalone, Sentinel, Redis Cluster

### Scenarios
Live cluster migration, cross-region disaster backup, multi-datacenter active-active architecture, cache data archiving & rollback.

需要我把这份文案压缩成 README 顶部**短摘要横幅版**（100字内，适合仓库首页第一眼展示）吗？
