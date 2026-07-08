//! utils/mod.rs - 工具模块入口
//!
//! 本模块包含各种工具函数，如日志初始化、哈希算法等。

mod log;
mod hash;

pub use log::init_logging;
pub use hash::{fnv_hash, fnv_hash_range, fnv_hash_str, fnv_hash_str_range};