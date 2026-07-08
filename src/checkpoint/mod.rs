//! checkpoint/mod.rs - Checkpoint 管理模块入口
//!
//! 本模块实现 Checkpoint 管理，支持读写检查点信息到目标 Redis。

pub mod manager;

pub use manager::*;