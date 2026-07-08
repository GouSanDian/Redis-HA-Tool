# 标准版

## 1.修改完成，等待测试
- restore

```
redis_syncer::syncer::syncer: src/syncer/syncer.rs:337: 发送 RDB 数据失败: 协议错误: RESTORE 失败 for key 'k15' (type: String): -ERR DUMP payload version or checksum are wrong
```


- "请求 PSYNC checkpoint 信息: HGETALL redis_ha_tool_checkpoint（扫描所有字段自动发现 master_replid） " 这个逻辑是错的，master_replid是个变量。详细逻辑见 @docs_dev/同步原理.md

- [ ] 目标端没有收到rdb全量同步的restore命令和aof增量同步的命令


## 2. 待修改
- [ ] cmd_black_list 加入PING，不生效，日志还会打印

- [ ] 测试：切换db

## 修改完成
- [x] checkpoint 信息写入
- [x] 没有按照配置文件里的配置设置日志吗？
- [x] 增量同步是一直进行的，不应该终止



## 企业版 稍后完成
- [ ] 支持防止循环复制
pub const CIRCLE_PREFIX_KEY: &str = "redis_ha_tool_circle_";

- [ ] 支持工具的集群模式
pub const ELECTION_PREFIX_KEY: &str = "redis_ha_tool_input_election_";

- [ ] 支持granfana监控
pub const DELAY_PREFIX_KEY: &str = "redis_ha_tool_delay_";
