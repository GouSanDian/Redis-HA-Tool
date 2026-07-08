//! cmd/mod.rs - 命令层模块入口
//!
//! 本模块实现 CLI 命令和 HTTP API。

pub mod syncer_cmd;
pub mod api;

pub use syncer_cmd::*;
pub use api::*;