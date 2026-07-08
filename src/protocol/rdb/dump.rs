//! protocol/rdb/dump.rs - DUMP 格式生成器
//!
//! 本文件实现 Redis DUMP 格式的序列化，用于 RESTORE 命令。
//! DUMP 格式: [version_byte][type_byte][length_encoding][data][crc64_checksum]
//!
//! 注意：raw_value 不包含类型字节，只包含长度编码和数据。
//! 类型字节需要在这里添加。

use bytes::{Bytes, BytesMut, BufMut};
use crate::protocol::rdb::{BinEntry, RdbType, crc64};

/// DUMP 格式版本号
/// 0 = Redis 2.6+
/// 1 = Redis 2.6+ with compression (未实现)
const DUMP_VERSION: u8 = 0;

/// 生成 DUMP 格式的序列化数据
///
/// # 参数
/// - entry: BinEntry 包含原始 RDB 编码数据
///
/// # 返回
/// DUMP 格式的字节数据，可用于 RESTORE 命令
///
/// # 格式
/// ```text
/// [version: 1 byte][type: 1 byte][length_encoding][data][crc64: 8 bytes]
/// ```
pub fn generate_dump_payload(entry: &BinEntry) -> Option<Bytes> {
    // 获取原始 RDB 编码数据
    let raw_value = entry.raw_value.as_ref()?;

    if raw_value.is_empty() {
        return None;
    }

    let mut dump_data = BytesMut::new();

    // 1. 添加版本号
    dump_data.put_u8(DUMP_VERSION);

    // 2. 添加类型字节
    dump_data.put_u8(entry.rdb_type as u8);

    // 3. 添加 RDB 编码数据（长度编码 + 数据）
    dump_data.extend_from_slice(raw_value);

    // 4. 计算并添加 CRC64 校验和
    let crc = crc64::crc64(&dump_data);
    dump_data.put_u64_le(crc);

    Some(dump_data.freeze())
}

/// 生成 DUMP 格式的序列化数据（带 TTL 信息）
///
/// 这个函数与 generate_dump_payload 相同，因为 TTL 是在 RESTORE 命令中指定的，
/// 不是在 DUMP 数据中。
pub fn generate_dump_payload_with_ttl(entry: &BinEntry) -> Option<Bytes> {
    generate_dump_payload(entry)
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::rdb::{RdbType, BinEntry};
    use bytes::Bytes;

    #[test]
    fn test_generate_dump_payload_string() {
        // 创建一个简单的 String 类型 BinEntry
        let key = Bytes::from("test_key");
        let value = Bytes::from("test_value");
        let rdb_type = RdbType::String;

        // 手动构造 raw_value (RDB 编码格式)
        // 注意：raw_value 不包含类型字节，只包含长度编码和数据
        // DUMP 格式: [version][type][length_encoding][data][crc64]
        let mut raw_value = BytesMut::new();
        // length (6-bit encoding, value = 10)
        raw_value.put_u8(value.len() as u8);
        raw_value.extend_from_slice(&value); // data

        let entry = BinEntry::new(0, key, value, rdb_type)
            .with_raw_value(raw_value.freeze());

        // 生成 DUMP payload
        let dump = generate_dump_payload(&entry);
        assert!(dump.is_some());

        let dump_data = dump.unwrap();

        // 验证格式
        // [version: 1][type: 1][length: 1][data: 10][crc64: 8] = 21 bytes
        assert_eq!(dump_data.len(), 1 + 1 + 1 + 10 + 8);

        // 验证版本号
        assert_eq!(dump_data[0], DUMP_VERSION);

        // 验证类型字节
        assert_eq!(dump_data[1], RdbType::String as u8);

        // 验证长度编码
        assert_eq!(dump_data[2], 10); // length = 10

        // 验证 CRC64 存在（最后 8 字节）
        let crc_bytes = &dump_data[dump_data.len() - 8..];
        let crc = u64::from_le_bytes(crc_bytes.try_into().unwrap());
        assert_ne!(crc, 0);
    }

    #[test]
    fn test_generate_dump_payload_empty_raw_value() {
        let key = Bytes::from("test_key");
        let value = Bytes::from("test_value");
        let rdb_type = RdbType::String;

        let entry = BinEntry::new(0, key, value, rdb_type);
        // 不设置 raw_value

        let dump = generate_dump_payload(&entry);
        assert!(dump.is_none());
    }

    #[test]
    fn test_dump_format_structure() {
        let key = Bytes::from("key");
        let value = Bytes::from("value");
        let rdb_type = RdbType::String;

        // 构造 raw_value（不包含类型字节）
        let mut raw_value = BytesMut::new();
        raw_value.put_u8(value.len() as u8); // length
        raw_value.extend_from_slice(&value); // data

        let entry = BinEntry::new(0, key, value, rdb_type)
            .with_raw_value(raw_value.freeze());

        let dump = generate_dump_payload(&entry).unwrap();

        // DUMP 格式: [version][type][length_encoding][data][crc64]
        // version = 1 byte
        // type = 1 byte
        // length_encoding = 1 byte (6-bit encoded length = 5)
        // data = 5 bytes ("value")
        // crc64 = 8 bytes
        // total = 16 bytes
        assert_eq!(dump.len(), 16);

        // 验证版本号
        assert_eq!(dump[0], 0);

        // 验证类型字节
        assert_eq!(dump[1], RdbType::String as u8);

        // 验证长度编码
        assert_eq!(dump[2], 5); // length = 5

        // 验证数据
        assert_eq!(&dump[3..8], b"value");

        // 验证 CRC64
        let expected_crc = crc64::crc64(&dump[0..8]);
        let actual_crc = u64::from_le_bytes(dump[8..16].try_into().unwrap());
        assert_eq!(expected_crc, actual_crc);
    }
}
