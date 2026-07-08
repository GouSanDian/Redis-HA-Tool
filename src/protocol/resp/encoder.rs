/// protocol/resp/encoder.rs - RESP 协议编码器
/// 
/// 本文件实现了 RESP 协议的编码功能，
/// 将 RespValue 转换为符合 RESP 协议格式的字节流。

use bytes::{Bytes, BytesMut, BufMut};
use crate::protocol::resp::RespValue;

/// RESP 协议编码器
/// 
/// 负责将 RespValue 转换为字节流。
pub struct RespEncoder;

impl RespEncoder {
    /// 编码 RESP 值为字节流
    /// 
    /// # 参数
    /// - value: 要编码的 RespValue
    /// 
    /// # 返回
    /// 编码后的字节流（BytesMut）
    /// 
    /// # 示例
    /// ```rust
    /// use redis_syncer::protocol::resp::{RespValue, RespEncoder};
    /// 
    /// let value = RespValue::simple_string("OK");
    /// let encoded = RespEncoder::encode(&value);
    /// assert_eq!(&encoded[..], b"+OK\r\n");
    /// ```
    pub fn encode(value: &RespValue) -> BytesMut {
        let mut buf = BytesMut::new();
        encode_value(value, &mut buf);
        buf
    }

    /// 编码多个 RESP 值为字节流
    /// 
    /// # 参数
    /// - values: 要编码的 RespValue 列表
    /// 
    /// # 返回
    /// 编码后的字节流（BytesMut）
    pub fn encode_batch(values: &[RespValue]) -> BytesMut {
        let mut buf = BytesMut::new();
        for value in values {
            encode_value(value, &mut buf);
        }
        buf
    }

    /// 编码为 Vec<u8>（方便某些场景使用）
    pub fn encode_to_vec(value: &RespValue) -> Vec<u8> {
        Self::encode(value).to_vec()
    }
}

/// 内部编码函数，递归编码 RESP 值
fn encode_value(value: &RespValue, buf: &mut BytesMut) {
    match value {
        // 简单字符串: +{string}\r\n
        RespValue::SimpleString(s) => {
            buf.put_u8(b'+');
            buf.put_slice(s);
            buf.put_slice(b"\r\n");
        }

        // 错误: -{error}\r\n
        RespValue::Error(e) => {
            buf.put_u8(b'-');
            buf.put_slice(e);
            buf.put_slice(b"\r\n");
        }

        // 整数: :{integer}\r\n
        RespValue::Integer(i) => {
            buf.put_u8(b':');
            buf.put_slice(i.to_string().as_bytes());
            buf.put_slice(b"\r\n");
        }

        // 批量字符串: ${length}\r\n{data}\r\n
        RespValue::BulkString(s) => {
            buf.put_u8(b'$');
            buf.put_slice(s.len().to_string().as_bytes());
            buf.put_slice(b"\r\n");
            buf.put_slice(s);
            buf.put_slice(b"\r\n");
        }

        // 数组: *{count}\r\n{elements}
        RespValue::Array(elements) => {
            buf.put_u8(b'*');
            buf.put_slice(elements.len().to_string().as_bytes());
            buf.put_slice(b"\r\n");
            for elem in elements {
                encode_value(elem, buf);
            }
        }

        // Null: $-1\r\n (使用 Bulk String 格式)
        RespValue::Null => {
            buf.put_slice(b"$-1\r\n");
        }
    }
}

/// 便捷方法：编码 Redis 命令
///
/// # 参数
/// - cmd: 命令名称
/// - args: 参数列表（Bytes）
///
/// # 返回
/// 编码后的 RESP 数组格式命令
pub fn encode_command(cmd: &str, args: &[Bytes]) -> BytesMut {
    let mut elements = Vec::with_capacity(1 + args.len());
    
    // 命令名
    elements.push(RespValue::BulkString(Bytes::copy_from_slice(cmd.as_bytes())));
    
    // 参数
    for arg in args {
        elements.push(RespValue::BulkString(arg.clone()));
    }
    
    RespEncoder::encode(&RespValue::Array(elements))
}

/// 便捷方法：编码 Redis 命令（字符串参数）
///
/// # 参数
/// - cmd: 命令名称
/// - args: 参数列表（字符串）
///
/// # 返回
/// 编码后的 RESP 数组格式命令
pub fn encode_command_str(cmd: &str, args: &[&str]) -> BytesMut {
    let args_bytes: Vec<Bytes> = args.iter()
        .map(|s| Bytes::copy_from_slice(s.as_bytes()))
        .collect();
    encode_command(cmd, &args_bytes)
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;

    /// 测试编码简单字符串
    #[test]
    fn test_encode_simple_string() {
        let value = RespValue::simple_string("OK");
        let encoded = RespEncoder::encode(&value);
        assert_eq!(&encoded[..], b"+OK\r\n");
    }

    /// 测试编码简单字符串（带空格）
    #[test]
    fn test_encode_simple_string_with_space() {
        let value = RespValue::simple_string("OK client");
        let encoded = RespEncoder::encode(&value);
        assert_eq!(&encoded[..], b"+OK client\r\n");
    }

    /// 测试编码错误
    #[test]
    fn test_encode_error() {
        let value = RespValue::error("ERR unknown command");
        let encoded = RespEncoder::encode(&value);
        assert_eq!(&encoded[..], b"-ERR unknown command\r\n");
    }

    /// 测试编码整数（正数）
    #[test]
    fn test_encode_integer_positive() {
        let value = RespValue::integer(42);
        let encoded = RespEncoder::encode(&value);
        assert_eq!(&encoded[..], b":42\r\n");
    }

    /// 测试编码整数（零）
    #[test]
    fn test_encode_integer_zero() {
        let value = RespValue::integer(0);
        let encoded = RespEncoder::encode(&value);
        assert_eq!(&encoded[..], b":0\r\n");
    }

    /// 测试编码整数（负数）
    #[test]
    fn test_encode_integer_negative() {
        let value = RespValue::integer(-1);
        let encoded = RespEncoder::encode(&value);
        assert_eq!(&encoded[..], b":-1\r\n");
    }

    /// 测试编码批量字符串
    #[test]
    fn test_encode_bulk_string() {
        let value = RespValue::bulk_string("hello");
        let encoded = RespEncoder::encode(&value);
        assert_eq!(&encoded[..], b"$5\r\nhello\r\n");
    }

    /// 测试编码空批量字符串
    #[test]
    fn test_encode_bulk_string_empty() {
        let value = RespValue::bulk_string("");
        let encoded = RespEncoder::encode(&value);
        assert_eq!(&encoded[..], b"$0\r\n\r\n");
    }

    /// 测试编码 Null
    #[test]
    fn test_encode_null() {
        let value = RespValue::null();
        let encoded = RespEncoder::encode(&value);
        assert_eq!(&encoded[..], b"$-1\r\n");
    }

    /// 测试编码空数组
    #[test]
    fn test_encode_array_empty() {
        let value = RespValue::empty_array();
        let encoded = RespEncoder::encode(&value);
        assert_eq!(&encoded[..], b"*0\r\n");
    }

    /// 测试编码数组（单个元素）
    #[test]
    fn test_encode_array_single_element() {
        let value = RespValue::array(vec![
            RespValue::bulk_string("foo"),
        ]);
        let encoded = RespEncoder::encode(&value);
        assert_eq!(&encoded[..], b"*1\r\n$3\r\nfoo\r\n");
    }

    /// 测试编码数组（多个元素）
    #[test]
    fn test_encode_array_multiple_elements() {
        let value = RespValue::array(vec![
            RespValue::bulk_string("SET"),
            RespValue::bulk_string("key"),
            RespValue::bulk_string("value"),
        ]);
        let encoded = RespEncoder::encode(&value);
        assert_eq!(&encoded[..], b"*3\r\n$3\r\nSET\r\n$3\r\nkey\r\n$5\r\nvalue\r\n");
    }

    /// 测试编码嵌套数组
    #[test]
    fn test_encode_nested_array() {
        let value = RespValue::array(vec![
            RespValue::array(vec![
                RespValue::integer(1),
                RespValue::integer(2),
            ]),
            RespValue::array(vec![
                RespValue::integer(3),
                RespValue::integer(4),
            ]),
        ]);
        let encoded = RespEncoder::encode(&value);
        assert_eq!(&encoded[..], b"*2\r\n*2\r\n:1\r\n:2\r\n*2\r\n:3\r\n:4\r\n");
    }

    /// 测试编码数组（包含 Null）
    #[test]
    fn test_encode_array_with_null() {
        let value = RespValue::array(vec![
            RespValue::null(),
            RespValue::integer(1),
        ]);
        let encoded = RespEncoder::encode(&value);
        assert_eq!(&encoded[..], b"*2\r\n$-1\r\n:1\r\n");
    }

    /// 测试编码混合类型数组
    #[test]
    fn test_encode_mixed_array() {
        let value = RespValue::array(vec![
            RespValue::simple_string("OK"),
            RespValue::integer(100),
            RespValue::bulk_string("data"),
        ]);
        let encoded = RespEncoder::encode(&value);
        assert_eq!(&encoded[..], b"*3\r\n+OK\r\n:100\r\n$4\r\ndata\r\n");
    }

    /// 测试批量编码
    #[test]
    fn test_encode_batch() {
        let values = vec![
            RespValue::simple_string("OK"),
            RespValue::integer(42),
        ];
        let encoded = RespEncoder::encode_batch(&values);
        assert_eq!(&encoded[..], b"+OK\r\n:42\r\n");
    }

    /// 测试编码为 Vec<u8>
    #[test]
    fn test_encode_to_vec() {
        let value = RespValue::simple_string("OK");
        let encoded = RespEncoder::encode_to_vec(&value);
        assert_eq!(encoded, b"+OK\r\n".to_vec());
    }

    /// 测试编码 Redis 命令（Bytes 参数）
    #[test]
    fn test_encode_command_bytes() {
        let cmd = encode_command("SET", &[
            Bytes::from("key"),
            Bytes::from("value"),
        ]);
        assert_eq!(&cmd[..], b"*3\r\n$3\r\nSET\r\n$3\r\nkey\r\n$5\r\nvalue\r\n");
    }

    /// 测试编码 Redis 命令（字符串参数）
    #[test]
    fn test_encode_command_str() {
        let cmd = encode_command_str("GET", &["key"]);
        assert_eq!(&cmd[..], b"*2\r\n$3\r\nGET\r\n$3\r\nkey\r\n");
    }

    /// 测试编码 Redis 命令（无参数）
    #[test]
    fn test_encode_command_no_args() {
        let cmd = encode_command_str("PING", &[]);
        assert_eq!(&cmd[..], b"*1\r\n$4\r\nPING\r\n");
    }

    /// 测试编码特殊字符
    #[test]
    fn test_encode_special_characters() {
        let value = RespValue::bulk_string("\r\n\t");
        let encoded = RespEncoder::encode(&value);
        assert_eq!(&encoded[..], b"$3\r\n\r\n\t\r\n");
    }

    /// 测试编码二进制数据
    #[test]
    fn test_encode_binary_data() {
        let data = Bytes::from(&[0x00, 0x01, 0xFF, 0xAB][..]);
        let value = RespValue::BulkString(data);
        let encoded = RespEncoder::encode(&value);
        
        // 验证格式正确
        assert!(encoded.starts_with(b"$4\r\n"));
        assert!(encoded.ends_with(b"\r\n"));
        // "$4\r\n" (4 bytes) + data (4 bytes) + "\r\n" (2 bytes) = 10 bytes
        assert_eq!(encoded.len(), 10);
        
        // 提取数据部分
        let data_part = &encoded[4..8]; // 跳过 "$4\r\n"
        assert_eq!(data_part, &[0x00, 0x01, 0xFF, 0xAB]);
    }
}