//! protocol/rdb/dump.rs - DUMP 格式生成器
//!
//! 本文件实现 Redis DUMP 格式的序列化，用于 RESTORE 命令。
//!
//! Redis DUMP 格式（经验证，基于 Redis 5.x/6.x dumpCommand）:
//!   [1 byte: object type]
//!   [RDB encoded value (length-encoded data, without type byte)]
//!   [2 bytes: RDB version, little-endian]
//!   [8 bytes: CRC64 checksum of type + encoded_value + version]
//!
//! 注意：raw_value 不包含类型字节，只包含长度编码和数据。
//! 类型字节需要在这里添加。CRC64 计算 type + raw_value + version 全部内容。

use bytes::{Bytes, BytesMut, BufMut};
use crate::protocol::rdb::{BinEntry, crc64};

/// 默认 RDB 版本号（RDB version 9 = Redis 5.x/6.x）
/// 如果无法从 RDB 解析中获取版本号，使用此默认值。
pub const DEFAULT_DUMP_RDB_VERSION: u16 = 9;

/// 生成 DUMP 格式的序列化数据
///
/// # 参数
/// - entry: BinEntry 包含原始 RDB 编码数据（raw_value = 长度编码 + 数据）
/// - rdb_version: RDB 版本号（例如 9, 10, 11），用于 DUMP 的 version 字段
///
/// # 返回
/// DUMP 格式的字节数据，可用于 RESTORE 命令
///
/// # 格式
/// ```text
/// [type: 1 byte][length_encoding][data][version: 2 bytes LE][crc64: 8 bytes]
/// CRC 计算范围: type + length_encoding + data + version (不含 CRC 自身)
/// ```
pub fn generate_dump_payload(entry: &BinEntry, rdb_version: u16) -> Option<Bytes> {
    // 获取原始 RDB 编码数据（长度编码 + 数据，不含 type byte）
    let raw_value = entry.raw_value.as_ref()?;

    if raw_value.is_empty() {
        return None;
    }

    let mut dump_data = BytesMut::new();

    // 1. 添加类型字节
    dump_data.put_u8(entry.rdb_type as u8);

    // 2. 添加 RDB 编码数据（长度编码 + 数据）
    dump_data.extend_from_slice(raw_value);

    // 3. 添加 RDB version（2 字节，小端序）
    dump_data.put_u16_le(rdb_version);

    // 4. 计算并添加 CRC64 校验和
    //    注意：CRC 覆盖 type + raw_value + version，这与 Redis 5.x/6.x 的行为一致
    let crc = crc64::crc64(&dump_data);
    dump_data.put_u64_le(crc);

    Some(dump_data.freeze())
}

/// 生成 DUMP 格式的序列化数据（带 TTL 信息）
///
/// 这个函数与 generate_dump_payload 相同，因为 TTL 是在 RESTORE 命令中指定的，
/// 不是在 DUMP 数据中。
pub fn generate_dump_payload_with_ttl(entry: &BinEntry, rdb_version: u16) -> Option<Bytes> {
    generate_dump_payload(entry, rdb_version)
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::rdb::{RdbType, BinEntry};
    use bytes::Bytes;

    /// 测试与 Redis 5.0.4 实际 DUMP 输出的一致性
    /// SET testkey "hello" → DUMP payload 应与 Redis 一致
    #[test]
    fn test_dump_matches_redis_504() {
        let key = Bytes::from("testkey");
        let value = Bytes::from("hello");
        let rdb_type = RdbType::String;

        // Redis 5.0.4 的 DUMP payload 中 raw_value = [0x05, 0x68, 0x65, 0x6c, 0x6c, 0x6f]
        // 即 [length=5]["hello"]
        let mut raw_value = BytesMut::new();
        raw_value.put_u8(value.len() as u8);
        raw_value.extend_from_slice(&value);

        let entry = BinEntry::new(0, key, value, rdb_type)
            .with_raw_value(raw_value.freeze());

        // 使用 RDB version 9 (与 Redis 5.0.4 一致)
        let dump = generate_dump_payload(&entry, 9).unwrap();

        // Redis 5.0.4 实际 DUMP: 000568656c6c6f0900 b3808eba31b243bb
        // [type=00][len=05][hello][version=0900][CRC64=b3808eba31b243bb]
        let expected: Vec<u8> = vec![
            0x00, // type = RDB_TYPE_STRING
            0x05, // length = 5
            0x68, 0x65, 0x6c, 0x6c, 0x6f, // "hello"
            0x09, 0x00, // RDB version 9 (LE)
            0xb3, 0x80, 0x8e, 0xba, 0x31, 0xb2, 0x43, 0xbb, // CRC64
        ];

        assert_eq!(dump.as_ref(), expected.as_slice(),
            "DUMP payload 应与 Redis 5.0.4 输出一致");
    }

    #[test]
    fn test_generate_dump_payload_string() {
        let key = Bytes::from("test_key");
        let value = Bytes::from("test_value");
        let rdb_type = RdbType::String;

        let mut raw_value = BytesMut::new();
        raw_value.put_u8(value.len() as u8);
        raw_value.extend_from_slice(&value);

        let entry = BinEntry::new(0, key, value, rdb_type)
            .with_raw_value(raw_value.freeze());

        let dump = generate_dump_payload(&entry, DEFAULT_DUMP_RDB_VERSION);
        assert!(dump.is_some());

        let dump_data = dump.unwrap();

        // 格式: [type:1][length:1][data:10][version:2][crc64:8] = 22 bytes
        assert_eq!(dump_data.len(), 1 + 1 + 10 + 2 + 8);

        // 验证类型字节
        assert_eq!(dump_data[0], RdbType::String as u8);

        // 验证长度编码
        assert_eq!(dump_data[1], 10);

        // 验证版本号 (RDB version 9 -> [0x09, 0x00])
        let ver_offset = 1 + 1 + 10; // after type + len + data
        assert_eq!(dump_data[ver_offset], 0x09);
        assert_eq!(dump_data[ver_offset + 1], 0x00);

        // 验证 CRC64 存在且不为 0
        let crc_bytes = &dump_data[dump_data.len() - 8..];
        let crc = u64::from_le_bytes(crc_bytes.try_into().unwrap());
        assert_ne!(crc, 0);

        // 验证 CRC 计算涵盖 type + raw_value + version
        let expected_crc = crc64::crc64(&dump_data[..dump_data.len() - 8]);
        assert_eq!(crc, expected_crc);
    }

    #[test]
    fn test_generate_dump_payload_empty_raw_value() {
        let key = Bytes::from("test_key");
        let value = Bytes::from("test_value");
        let rdb_type = RdbType::String;

        let entry = BinEntry::new(0, key, value, rdb_type);
        let dump = generate_dump_payload(&entry, DEFAULT_DUMP_RDB_VERSION);
        assert!(dump.is_none());
    }

    #[test]
    fn test_dump_format_structure() {
        let key = Bytes::from("key");
        let value = Bytes::from("value");
        let rdb_type = RdbType::String;

        let mut raw_value = BytesMut::new();
        raw_value.put_u8(value.len() as u8);
        raw_value.extend_from_slice(&value);

        let entry = BinEntry::new(0, key, value, rdb_type)
            .with_raw_value(raw_value.freeze());

        let dump = generate_dump_payload(&entry, DEFAULT_DUMP_RDB_VERSION).unwrap();

        // 格式: [type:1][length:1][data:5][version:2][crc64:8] = 17 bytes
        assert_eq!(dump.len(), 17);

        // 验证类型字节
        assert_eq!(dump[0], RdbType::String as u8);

        // 验证长度编码
        assert_eq!(dump[1], 5);

        // 验证数据
        assert_eq!(&dump[2..7], b"value");

        // 验证版本号
        assert_eq!(dump[7], 0x09);
        assert_eq!(dump[8], 0x00);

        // 验证 CRC64: 计算 type+raw_value+version (bytes 0..9)
        let expected_crc = crc64::crc64(&dump[0..9]);
        let actual_crc = u64::from_le_bytes(dump[9..17].try_into().unwrap());
        assert_eq!(expected_crc, actual_crc);
    }
}
