/// protocol/resp/value.rs - RESP 协议值类型定义
/// 
/// 本文件定义了 RESP（Redis Serialization Protocol）协议的各种数据类型，
/// 包括简单字符串、错误、整数、批量字符串、数组等。

use bytes::Bytes;
use std::fmt;

/// RESP 协议值类型
/// 
/// RESP 协议支持 5 种基本类型：
/// - Simple String：简单字符串（以 `+` 开头）
/// - Error：错误消息（以 `-` 开头）
/// - Integer：整数（以 `:` 开头）
/// - Bulk String：批量字符串（以 `$` 开头，带长度前缀）
/// - Array：数组（以 `*` 开头，带计数前缀）
/// 
/// 另外，Bulk String 和 Array 可以有 Null 表示（长度为 -1）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RespValue {
    /// 简单字符串
    /// 
    /// 格式: `+{string}\r\n`
    /// 例如: `+OK\r\n`
    SimpleString(Bytes),

    /// 错误消息
    /// 
    /// 格式: `-{error}\r\n`
    /// 例如: `-ERR unknown command\r\n`
    Error(Bytes),

    /// 整数
    /// 
    /// 格式: `:{integer}\r\n`
    /// 例如: `:1000\r\n`
    Integer(i64),

    /// 批量字符串
    /// 
    /// 格式: `${length}\r\n{data}\r\n`
    /// 例如: `$6\r\nfoobar\r\n`
    /// 
    /// 空批量字符串: `$0\r\n\r\n`
    BulkString(Bytes),

    /// 数组
    /// 
    /// 格式: `*{count}\r\n{elements}\r\n`
    /// 例如: `*2\r\n$3\r\nfoo\r\n$3\r\nbar\r\n`
    Array(Vec<RespValue>),

    /// Null 值
    /// 
    /// Bulk String Null: `$-1\r\n`
    /// Array Null: `*-1\r\n`
    Null,
}

impl RespValue {
    /// 判断是否为简单字符串
    pub fn is_simple_string(&self) -> bool {
        matches!(self, RespValue::SimpleString(_))
    }

    /// 判断是否为错误
    pub fn is_error(&self) -> bool {
        matches!(self, RespValue::Error(_))
    }

    /// 判断是否为整数
    pub fn is_integer(&self) -> bool {
        matches!(self, RespValue::Integer(_))
    }

    /// 判断是否为批量字符串
    pub fn is_bulk_string(&self) -> bool {
        matches!(self, RespValue::BulkString(_))
    }

    /// 判断是否为数组
    pub fn is_array(&self) -> bool {
        matches!(self, RespValue::Array(_))
    }

    /// 判断是否为 Null
    pub fn is_null(&self) -> bool {
        matches!(self, RespValue::Null)
    }

    /// 获取简单字符串内容（如果类型匹配）
    pub fn as_simple_string(&self) -> Option<&Bytes> {
        match self {
            RespValue::SimpleString(s) => Some(s),
            _ => None,
        }
    }

    /// 获取错误内容（如果类型匹配）
    pub fn as_error(&self) -> Option<&Bytes> {
        match self {
            RespValue::Error(e) => Some(e),
            _ => None,
        }
    }

    /// 获取整数内容（如果类型匹配）
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            RespValue::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// 获取批量字符串内容（如果类型匹配）
    pub fn as_bulk_string(&self) -> Option<&Bytes> {
        match self {
            RespValue::BulkString(s) => Some(s),
            _ => None,
        }
    }

    /// 获取数组内容（如果类型匹配）
    pub fn as_array(&self) -> Option<&Vec<RespValue>> {
        match self {
            RespValue::Array(arr) => Some(arr),
            _ => None,
        }
    }

    /// 从字符串创建简单字符串
    pub fn simple_string(s: &str) -> Self {
        RespValue::SimpleString(Bytes::copy_from_slice(s.as_bytes()))
    }

    /// 从字符串创建错误
    pub fn error(s: &str) -> Self {
        RespValue::Error(Bytes::copy_from_slice(s.as_bytes()))
    }

    /// 从整数创建整数
    pub fn integer(i: i64) -> Self {
        RespValue::Integer(i)
    }

    /// 从字符串创建批量字符串
    pub fn bulk_string(s: &str) -> Self {
        RespValue::BulkString(Bytes::copy_from_slice(s.as_bytes()))
    }

    /// 从 Bytes 创建批量字符串
    pub fn bulk_string_from_bytes(b: Bytes) -> Self {
        RespValue::BulkString(b)
    }

    /// 创建空数组
    pub fn empty_array() -> Self {
        RespValue::Array(Vec::new())
    }

    /// 创建数组
    pub fn array(elements: Vec<RespValue>) -> Self {
        RespValue::Array(elements)
    }

    /// 创建 Null
    pub fn null() -> Self {
        RespValue::Null
    }

    /// 获取 RESP 类型标记字符
    pub fn type_marker(&self) -> char {
        match self {
            RespValue::SimpleString(_) => '+',
            RespValue::Error(_) => '-',
            RespValue::Integer(_) => ':',
            RespValue::BulkString(_) => '$',
            RespValue::Array(_) => '*',
            RespValue::Null => '$', // Null 使用 Bulk String 格式 $-1\r\n
        }
    }

    /// 判断是否为 OK 响应
    pub fn is_ok(&self) -> bool {
        match self {
            RespValue::SimpleString(s) => s == "OK",
            _ => false,
        }
    }

    /// 尝试解析为 Redis 命令
    /// 
    /// 如果是数组类型且第一个元素是批量字符串（命令名），
    /// 返回命令名和参数列表。
    pub fn as_command(&self) -> Option<(String, Vec<Bytes>)> {
        match self {
            RespValue::Array(elements) if elements.len() > 0 => {
                // 第一个元素是命令名
                let cmd_name = elements[0].as_bulk_string()?;
                let cmd = String::from_utf8_lossy(cmd_name).to_string();
                
                // 剩余元素是参数
                let args: Vec<Bytes> = elements[1..]
                    .iter()
                    .filter_map(|e| e.as_bulk_string().cloned())
                    .collect();
                
                Some((cmd, args))
            }
            _ => None,
        }
    }
}

/// 自定义 Display 实现，用于调试输出
impl fmt::Display for RespValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RespValue::SimpleString(s) => {
                write!(f, "+{}", String::from_utf8_lossy(s))
            }
            RespValue::Error(e) => {
                write!(f, "-{}", String::from_utf8_lossy(e))
            }
            RespValue::Integer(i) => {
                write!(f, ":{}", i)
            }
            RespValue::BulkString(s) => {
                if s.is_empty() {
                    write!(f, "$0")
                } else {
                    write!(f, "${} ({})", s.len(), String::from_utf8_lossy(s))
                }
            }
            RespValue::Array(arr) => {
                write!(f, "*{} [", arr.len());
                for (i, elem) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ");
                    }
                    write!(f, "{}", elem);
                }
                write!(f, "]")
            }
            RespValue::Null => {
                write!(f, "$-1 (Null)")
            }
        }
    }
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;

    /// 测试简单字符串创建和判断
    #[test]
    fn test_simple_string() {
        let value = RespValue::simple_string("OK");
        assert!(value.is_simple_string());
        assert!(!value.is_error());
        assert_eq!(value.as_simple_string(), Some(&Bytes::from("OK")));
        assert!(value.is_ok());
        assert_eq!(value.type_marker(), '+');
    }

    /// 测试错误创建和判断
    #[test]
    fn test_error() {
        let value = RespValue::error("ERR unknown");
        assert!(value.is_error());
        assert!(!value.is_simple_string());
        assert_eq!(value.as_error(), Some(&Bytes::from("ERR unknown")));
        assert_eq!(value.type_marker(), '-');
    }

    /// 测试整数创建和判断
    #[test]
    fn test_integer() {
        let value = RespValue::integer(42);
        assert!(value.is_integer());
        assert_eq!(value.as_integer(), Some(42));
        assert_eq!(value.type_marker(), ':');
        
        // 测试负整数
        let neg_value = RespValue::integer(-1);
        assert_eq!(neg_value.as_integer(), Some(-1));
    }

    /// 测试批量字符串创建和判断
    #[test]
    fn test_bulk_string() {
        let value = RespValue::bulk_string("hello");
        assert!(value.is_bulk_string());
        assert_eq!(value.as_bulk_string(), Some(&Bytes::from("hello")));
        assert_eq!(value.type_marker(), '$');
        
        // 测试空批量字符串
        let empty = RespValue::bulk_string("");
        assert!(empty.is_bulk_string());
        assert_eq!(empty.as_bulk_string(), Some(&Bytes::from("")));
    }

    /// 测试数组创建和判断
    #[test]
    fn test_array() {
        let value = RespValue::array(vec![
            RespValue::bulk_string("SET"),
            RespValue::bulk_string("key"),
            RespValue::bulk_string("value"),
        ]);
        assert!(value.is_array());
        assert_eq!(value.as_array().unwrap().len(), 3);
        assert_eq!(value.type_marker(), '*');
        
        // 测试空数组
        let empty = RespValue::empty_array();
        assert!(empty.is_array());
        assert_eq!(empty.as_array().unwrap().len(), 0);
    }

    /// 测试 Null 创建和判断
    #[test]
    fn test_null() {
        let value = RespValue::null();
        assert!(value.is_null());
        assert!(value.as_bulk_string().is_none());
    }

    /// 测试嵌套数组
    #[test]
    fn test_nested_array() {
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
        assert!(value.is_array());
        assert_eq!(value.as_array().unwrap().len(), 2);
    }

    /// 测试 Display 实现
    #[test]
    fn test_display() {
        let value = RespValue::simple_string("OK");
        assert_eq!(format!("{}", value), "+OK");
        
        let value = RespValue::error("ERR test");
        assert_eq!(format!("{}", value), "-ERR test");
        
        let value = RespValue::integer(42);
        assert_eq!(format!("{}", value), ":42");
        
        let value = RespValue::bulk_string("hello");
        assert_eq!(format!("{}", value), "$5 (hello)");
        
        let value = RespValue::null();
        assert_eq!(format!("{}", value), "$-1 (Null)");
    }

    /// 测试解析为 Redis 命令
    #[test]
    fn test_as_command() {
        let value = RespValue::array(vec![
            RespValue::bulk_string("SET"),
            RespValue::bulk_string("key"),
            RespValue::bulk_string("value"),
        ]);
        
        let (cmd, args) = value.as_command().unwrap();
        assert_eq!(cmd, "SET");
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], Bytes::from("key"));
        assert_eq!(args[1], Bytes::from("value"));
    }

    /// 测试解析命令失败（非数组）
    #[test]
    fn test_as_command_invalid_type() {
        let value = RespValue::simple_string("OK");
        assert!(value.as_command().is_none());
    }

    /// 测试解析命令失败（第一个元素不是 BulkString）
    #[test]
    fn test_as_command_invalid_first_element() {
        let value = RespValue::array(vec![
            RespValue::integer(1),  // 无效的命令名
        ]);
        assert!(value.as_command().is_none());
    }

    /// 测试 Bytes 创建批量字符串
    #[test]
    fn test_bulk_string_from_bytes() {
        let bytes = Bytes::from("test");
        let value = RespValue::bulk_string_from_bytes(bytes.clone());
        assert_eq!(value.as_bulk_string(), Some(&bytes));
    }

    /// 测试 PartialEq
    #[test]
    fn test_equality() {
        let v1 = RespValue::simple_string("OK");
        let v2 = RespValue::simple_string("OK");
        let v3 = RespValue::simple_string("ERROR");
        
        assert_eq!(v1, v2);
        assert_ne!(v1, v3);
    }
}