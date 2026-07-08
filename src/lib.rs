/// lib.rs - 库根，声明所有子模块
/// 
/// 本文件是 redis-ha-tool 库的入口点，声明所有子模块供外部使用。

// 公开模块
pub mod config;       // 配置系统
pub mod protocol;     // 协议实现（RESP, RDB）
pub mod utils;        // 工具函数
pub mod error;        // 错误类型定义

// 内部模块
pub mod store;        // 存储系统
pub mod filter;       // 过滤系统
pub mod checkpoint;   // Checkpoint 管理
pub mod syncer;       // 同步器
pub mod cmd;          // 命令层

// 阶段四模块
pub mod cluster;      // 集群选举
pub mod metric;       // 监控指标