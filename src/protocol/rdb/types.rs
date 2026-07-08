//! protocol/rdb/types.rs - RDB 类型常量和数据结构
//!
//! 本文件定义 RDB 格式的类型常量和 BinEntry 结构。

use bytes::Bytes;
use std::time::SystemTime;

/// RDB 文件魔数
pub const RDB_MAGIC: &[u8] = b"REDIS";

/// RDB 版本号（Redis 6.x）
pub const RDB_VERSION_6: u32 = 9;
pub const RDB_VERSION_7: u32 = 10;
pub const RDB_VERSION_8: u32 = 11;

/// RDB 操作码（Opcode）
///
/// 参考：https://rdb.fnshy.com/
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RdbOpcode {
    /// AUX 字段（辅助数据）
    Aux = 0xFA,
    
    /// ResizeDB（数据库大小）
    ResizeDb = 0xFB,
    
    /// Expire时间（毫秒）
    ExpireMs = 0xFC,
    
    /// Expire时间（秒）
    ExpireS = 0xFD,
    
    /// SelectDB（选择数据库）
    SelectDb = 0xFE,
    
    /// EOF（文件结束）
    Eof = 0xFF,
    
    /// Module数据
    Module = 0xF2,
    
    /// ModuleAux 数据
    ModuleAux = 0xF3,
    
    /// Function数据（Redis 7+）
    Function = 0xF4,
    
    /// FunctionAux 数据（Redis 7+）
    FunctionAux = 0xF5,
}

/// RDB 数据类型
///
/// 定义 Redis 支持的各种数据类型编码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RdbType {
    /// String（字符串）
    String = 0,
    
    /// List（列表）
    List = 1,
    
    /// Set（集合）
    Set = 2,
    
    /// Sorted Set（有序集合）
    SortedSet = 3,
    
    /// Hash（哈希）
    Hash = 4,
    
    /// Zipmap（压缩哈希）
    Zipmap = 5,
    
    /// Ziplist（压缩列表）
    Ziplist = 6,
    
    /// Intset（整数集合）
    Intset = 7,
    
    /// Sorted Set in Ziplist（压缩有序集合）
    SortedSetZiplist = 8,
    
    /// Hashmap as Ziplist（压缩哈希）
    HashmapZiplist = 9,
    
    /// Listpack（Redis 7+）
    Listpack = 10,
    
    /// Hash as Listpack（Redis 7+）
    HashListpack = 11,
    
    /// Sorted Set as Listpack（Redis 7+）
    SortedSetListpack = 12,
    
    /// Stream as Listpack（Redis 7+）
    StreamListpack = 13,
    
    /// Module（模块数据）
    Module = 14,
    
    /// Module 2（模块数据）
    Module2 = 15,
    
    /// Stream as Listpack 2（Redis 7+）
    StreamListpack2 = 16,
    
    /// Set as Listpack（Redis 7+）
    SetListpack = 17,
    
    /// Function（Redis 7+）
    Function2 = 18,
}

impl RdbType {
    /// 从字节解析 RdbType
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(RdbType::String),
            1 => Some(RdbType::List),
            2 => Some(RdbType::Set),
            3 => Some(RdbType::SortedSet),
            4 => Some(RdbType::Hash),
            5 => Some(RdbType::Zipmap),
            6 => Some(RdbType::Ziplist),
            7 => Some(RdbType::Intset),
            8 => Some(RdbType::SortedSetZiplist),
            9 => Some(RdbType::HashmapZiplist),
            10 => Some(RdbType::Listpack),
            11 => Some(RdbType::HashListpack),
            12 => Some(RdbType::SortedSetListpack),
            13 => Some(RdbType::StreamListpack),
            14 => Some(RdbType::Module),
            15 => Some(RdbType::Module2),
            16 => Some(RdbType::StreamListpack2),
            17 => Some(RdbType::SetListpack),
            18 => Some(RdbType::Function2),
            _ => None,
        }
    }
    
    /// 是否为字符串类型
    pub fn is_string(&self) -> bool {
        matches!(self, RdbType::String)
    }
    
    /// 是否为列表类型
    pub fn is_list(&self) -> bool {
        matches!(self, RdbType::List | RdbType::Ziplist | RdbType::Listpack)
    }
    
    /// 是否为集合类型
    pub fn is_set(&self) -> bool {
        matches!(self, RdbType::Set | RdbType::Intset | RdbType::SetListpack)
    }
    
    /// 是否为有序集合类型
    pub fn is_sorted_set(&self) -> bool {
        matches!(self, 
            RdbType::SortedSet | 
            RdbType::SortedSetZiplist | 
            RdbType::SortedSetListpack
        )
    }
    
    /// 是否为哈希类型
    pub fn is_hash(&self) -> bool {
        matches!(self, 
            RdbType::Hash | 
            RdbType::Zipmap | 
            RdbType::HashmapZiplist | 
            RdbType::HashListpack
        )
    }
    
    /// 是否为压缩编码
    pub fn is_compressed(&self) -> bool {
        matches!(self,
            RdbType::Zipmap |
            RdbType::Ziplist |
            RdbType::Intset |
            RdbType::SortedSetZiplist |
            RdbType::HashmapZiplist |
            RdbType::Listpack |
            RdbType::HashListpack |
            RdbType::SortedSetListpack |
            RdbType::StreamListpack |
            RdbType::SetListpack
        )
    }
}

/// RDB BinEntry（二进制条目）
///
/// 表示从 RDB 文件解析出的一个键值对条目。
#[derive(Debug, Clone)]
pub struct BinEntry {
    /// 数据库编号
    pub db: u32,

    /// 键（使用 Bytes 零拷贝）
    pub key: Bytes,

    /// 值（使用 Bytes 零拷贝）
    /// 对于 String 类型，这是解码后的字符串值
    /// 对于其他类型，这是解析后的值（可能不完整）
    pub value: Bytes,

    /// 原始 RDB 编码数据（用于生成 DUMP 格式）
    /// 包含完整的 RDB 编码值，可用于 RESTORE 命令
    pub raw_value: Option<Bytes>,

    /// 过期时间（可选）
    pub expire: Option<SystemTime>,

    /// RDB 数据类型
    pub rdb_type: RdbType,
}

impl BinEntry {
    /// 创建新的 BinEntry
    pub fn new(db: u32, key: Bytes, value: Bytes, rdb_type: RdbType) -> Self {
        BinEntry {
            db,
            key,
            value,
            raw_value: None,
            expire: None,
            rdb_type,
        }
    }

    /// 创建带有原始数据的 BinEntry
    pub fn with_raw_value(mut self, raw_value: Bytes) -> Self {
        self.raw_value = Some(raw_value);
        self
    }
    
    /// 设置过期时间
    pub fn with_expire(mut self, expire: SystemTime) -> Self {
        self.expire = Some(expire);
        self
    }
    
    /// 设置过期时间（毫秒）
    pub fn with_expire_ms(mut self, expire_ms: u64) -> Self {
        self.expire = Some(
            SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(expire_ms)
        );
        self
    }
    
    /// 设置过期时间（秒）
    pub fn with_expire_s(mut self, expire_s: u64) -> Self {
        self.expire = Some(
            SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(expire_s)
        );
        self
    }
}

/// RDB AUX 字段
///
/// 存储辅助信息（如 Redis 版本、创建时间等）。
#[derive(Debug, Clone)]
pub struct RdbAuxField {
    /// 字段键
    pub key: Bytes,
    
    /// 字段值
    pub value: Bytes,
}

impl RdbAuxField {
    /// 创建新的 AUX 字段
    pub fn new(key: Bytes, value: Bytes) -> Self {
        RdbAuxField { key, value }
    }
}

/// RDB 解析器状态
///
/// 跟踪解析过程中的状态信息。
#[derive(Debug, Clone)]
pub struct RdbParserState {
    /// 当前数据库编号
    pub current_db: u32,
    
    /// 当前键的过期时间
    pub current_expire: Option<SystemTime>,
    
    /// 是否遇到 EOF
    pub eof_reached: bool,
    
    /// RDB 版本
    pub version: u32,
    
    /// Redis 运行 ID（从 AUX 字段读取）
    pub run_id: Option<String>,
}

impl Default for RdbParserState {
    fn default() -> Self {
        RdbParserState {
            current_db: 0,
            current_expire: None,
            eof_reached: false,
            version: 0,
            run_id: None,
        }
    }
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;
    
    /// 测试 RdbType 解析
    #[test]
    fn test_rdb_type_from_byte() {
        assert_eq!(RdbType::from_byte(0), Some(RdbType::String));
        assert_eq!(RdbType::from_byte(1), Some(RdbType::List));
        assert_eq!(RdbType::from_byte(2), Some(RdbType::Set));
        assert_eq!(RdbType::from_byte(4), Some(RdbType::Hash));
        assert_eq!(RdbType::from_byte(255), None);
    }
    
    /// 测试 RdbType 类型判断
    #[test]
    fn test_rdb_type_checks() {
        let string_type = RdbType::String;
        assert!(string_type.is_string());
        assert!(!string_type.is_list());
        
        let hash_type = RdbType::Hash;
        assert!(hash_type.is_hash());
        assert!(!hash_type.is_compressed());
        
        let ziplist_type = RdbType::Ziplist;
        assert!(ziplist_type.is_list());
        assert!(ziplist_type.is_compressed());
    }
    
    /// 测试 BinEntry 创建
    #[test]
    fn test_bin_entry() {
        let entry = BinEntry::new(
            0,
            Bytes::from("key"),
            Bytes::from("value"),
            RdbType::String,
        );
        
        assert_eq!(entry.db, 0);
        assert_eq!(entry.key.as_ref(), b"key");
        assert_eq!(entry.value.as_ref(), b"value");
        assert!(entry.expire.is_none());
    }
    
    /// 测试 BinEntry 过期时间设置
    #[test]
    fn test_bin_entry_with_expire() {
        let entry = BinEntry::new(
            0,
            Bytes::from("key"),
            Bytes::from("value"),
            RdbType::String,
        ).with_expire_ms(1234567890);
        
        assert!(entry.expire.is_some());
        
        let expire_time = entry.expire.unwrap();
        let since_epoch = expire_time.duration_since(SystemTime::UNIX_EPOCH).unwrap();
        assert_eq!(since_epoch.as_millis(), 1234567890);
    }
    
    /// 测试 RdbParserState 默认值
    #[test]
    fn test_rdb_parser_state_default() {
        let state = RdbParserState::default();
        
        assert_eq!(state.current_db, 0);
        assert!(state.current_expire.is_none());
        assert!(!state.eof_reached);
        assert_eq!(state.version, 0);
        assert!(state.run_id.is_none());
    }
}