//! protocol/rdb/parser.rs - RDB 文件解析器
//!
//! 本文件实现 RDB 文件的解析逻辑。
//!
//! 注意：这是一个简化实现，支持基本的 RDB 格式。
//! 完整实现需要处理所有压缩编码和特殊类型。

use bytes::{Bytes, BytesMut};
use tokio::io::{AsyncRead, AsyncReadExt};
use crate::error::{SyncError, Result};
use crate::protocol::rdb::{RdbType, RdbOpcode, BinEntry, RdbAuxField, RdbParserState, RDB_MAGIC};

/// RDB 解析器
///
/// 解析 RDB 文件并通过回调函数发送解析出的 BinEntry。
pub struct RdbParser<R> {
    /// 输入流
    reader: R,
    /// 解析状态
    state: RdbParserState,
}

impl<R: AsyncRead + Unpin> RdbParser<R> {
    /// 创建新的 RDB 解析器
    pub fn new(reader: R) -> Self {
        RdbParser {
            reader,
            state: RdbParserState::default(),
        }
    }
    
    /// 解析 RDB 文件
    ///
    /// 通过回调函数发送每个解析出的 BinEntry。
    ///
    /// # 参数
    /// - callback: 处理 BinEntry 的回调函数
    ///
    /// # 返回
    /// 成功或错误
    pub async fn parse<F>(&mut self, mut callback: F) -> Result<Vec<RdbAuxField>>
    where
        F: FnMut(BinEntry) -> Result<()>,
    {
        // 读取并验证魔数
        self.read_magic().await?;
        
        // 读取版本号
        self.state.version = self.read_version().await?;
        
        // 存储 AUX 字段
        let aux_fields = Vec::new();
        
        // 解析主体
        while !self.state.eof_reached {
            let opcode = self.read_byte().await?;
            
            match opcode {
                0xFA => { // AUX
                    let aux = self.read_aux_field().await?;
                    // 处理特定的 AUX 字段
                    if aux.key.as_ref() == b"repl-id" {
                        self.state.run_id = Some(String::from_utf8_lossy(&aux.value).to_string());
                    }
                }
                
                0xFB => { // ResizeDB
                    // 读取 db_size 和 expires_size，使用内联读取
                    let first_byte = self.read_byte().await?;
                    let length_type = first_byte >> 6;
                    let _db_size = match length_type {
                        0 => first_byte as usize & 0x3F,
                        1 => {
                            let second_byte = self.read_byte().await?;
                            ((first_byte as usize & 0x3F) << 8) | second_byte as usize
                        }
                        2 => self.read_u32().await? as usize,
                        _ => return Err(SyncError::Corrupted("ResizeDB db_size 特殊编码不支持".into())),
                    };

                    let first_byte2 = self.read_byte().await?;
                    let length_type2 = first_byte2 >> 6;
                    let _expires_size = match length_type2 {
                        0 => first_byte2 as usize & 0x3F,
                        1 => {
                            let second_byte = self.read_byte().await?;
                            ((first_byte2 as usize & 0x3F) << 8) | second_byte as usize
                        }
                        2 => self.read_u32().await? as usize,
                        _ => return Err(SyncError::Corrupted("ResizeDB expires_size 特殊编码不支持".into())),
                    };
                }
                
                0xFC => { // ExpireMs
                    let expire_ms = self.read_u64().await?;
                    self.state.current_expire = Some(
                        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(expire_ms)
                    );
                }
                
                0xFD => { // ExpireS
                    let expire_s = self.read_u32().await?;
                    self.state.current_expire = Some(
                        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(expire_s as u64)
                    );
                }
                
                0xFE => { // SelectDB
                    // 读取数据库编号，使用 read_length 处理特殊编码
                    let first_byte = self.read_byte().await?;
                    let length_type = first_byte >> 6;
                    let db = match length_type {
                        0 => first_byte as usize & 0x3F,
                        1 => {
                            let second_byte = self.read_byte().await?;
                            ((first_byte as usize & 0x3F) << 8) | second_byte as usize
                        }
                        2 => self.read_u32().await? as usize,
                        _ => return Err(SyncError::Corrupted("SelectDB 特殊编码不支持".into())),
                    };
                    self.state.current_db = db as u32;
                    self.state.current_expire = None;
                }
                
                0xFF => { // EOF
                    self.state.eof_reached = true;
                    // 读取校验和（可选）
                    break;
                }
                
                _ => {
                    // 尝试解析为数据类型
                    if let Some(rdb_type) = RdbType::from_byte(opcode) {
                        let entry = self.read_key_value(rdb_type).await?;
                        callback(entry)?;
                    } else {
                        // 未知的 opcode
                        tracing::warn!("未知的 RDB opcode: {}", opcode);
                    }
                }
            }
        }
        
        Ok(aux_fields)
    }
    
    /// 读取魔数
    async fn read_magic(&mut self) -> Result<()> {
        let mut magic = [0u8; 5];
        self.reader.read_exact(&mut magic).await?;
        
        if &magic != RDB_MAGIC {
            return Err(SyncError::Corrupted(format!(
                "无效的 RDB 魔数: {}",
                String::from_utf8_lossy(&magic)
            )));
        }
        
        Ok(())
    }
    
    /// 读取版本号
    async fn read_version(&mut self) -> Result<u32> {
        let mut version_bytes = [0u8; 4];
        self.reader.read_exact(&mut version_bytes).await?;
        
        let version_str = String::from_utf8_lossy(&version_bytes);
        let version: u32 = version_str.parse()
            .map_err(|_| SyncError::Corrupted(format!("无效的 RDB 版本: {}", version_str)))?;
        
        Ok(version)
    }
    
    /// 读取一个字节
    async fn read_byte(&mut self) -> Result<u8> {
        let byte = self.reader.read_u8().await?;
        Ok(byte)
    }
    
    /// 读取长度编码值
    ///
    /// 支持多种长度编码格式。
    async fn read_length(&mut self) -> Result<usize> {
        let first_byte = self.read_byte().await?;

        let length_type = first_byte >> 6;

        match length_type {
            0 => { // 6 bit length
                Ok(first_byte as usize & 0x3F)
            }
            1 => { // 14 bit length
                let second_byte = self.read_byte().await?;
                Ok(((first_byte as usize & 0x3F) << 8) | second_byte as usize)
            }
            2 => { // 32 bit length
                let length = self.read_u32().await?;
                Ok(length as usize)
            }
            3 => { // Special encoding - this is NOT a length, it's an encoded integer
                // Return the special encoding indicator
                // The caller needs to handle this
                Err(SyncError::Corrupted(format!("Special encoding encountered: {}", first_byte)))
            }
            _ => unreachable!(),
        }
    }

    /// 读取长度编码值，处理特殊编码
    ///
    /// 返回 (长度, 是否是特殊编码, 编码值)
    async fn read_length_with_encoding(&mut self) -> Result<(usize, bool, u64)> {
        let first_byte = self.read_byte().await?;

        let length_type = first_byte >> 6;

        match length_type {
            0 => { // 6 bit length
                Ok((first_byte as usize & 0x3F, false, 0))
            }
            1 => { // 14 bit length
                let second_byte = self.read_byte().await?;
                let len = ((first_byte as usize & 0x3F) << 8) | second_byte as usize;
                Ok((len, false, 0))
            }
            2 => { // 32 bit length
                let length = self.read_u32().await?;
                Ok((length as usize, false, 0))
            }
            3 => { // Special encoding
                let encoding = first_byte & 0x3F;
                let value = match encoding {
                    0 => self.read_byte().await? as u64, // 8 bit integer
                    1 => self.read_u16().await? as u64,  // 16 bit integer
                    2 => self.read_u32().await? as u64,  // 32 bit integer
                    _ => return Err(SyncError::Corrupted(format!("未知的特殊编码类型: {}", encoding))),
                };
                Ok((0, true, value))
            }
            _ => unreachable!(),
        }
    }

    /// 读取 u16 (little endian)
    async fn read_u16(&mut self) -> Result<u16> {
        let value = self.reader.read_u16_le().await?;
        Ok(value)
    }
    
    /// 读取字符串（支持特殊编码的整数）
    async fn read_string(&mut self) -> Result<Bytes> {
        // 先尝试读取长度
        let first_byte = self.read_byte().await?;
        let length_type = first_byte >> 6;

        match length_type {
            0 => { // 6 bit length
                let len = first_byte as usize & 0x3F;
                self.read_string_raw(len).await
            }
            1 => { // 14 bit length
                let second_byte = self.read_byte().await?;
                let len = ((first_byte as usize & 0x3F) << 8) | second_byte as usize;
                self.read_string_raw(len).await
            }
            2 => { // 32 bit length
                let len = self.read_u32().await? as usize;
                self.read_string_raw(len).await
            }
            3 => { // Special encoding - integer as string
                let encoding = first_byte & 0x3F;
                let value = match encoding {
                    0 => { // 8 bit integer
                        let b = self.read_byte().await?;
                        format!("{}", b as i8)
                    }
                    1 => { // 16 bit integer
                        let v = self.read_u16().await?;
                        format!("{}", v as i16)
                    }
                    2 => { // 32 bit integer
                        let v = self.read_u32().await?;
                        format!("{}", v as i32)
                    }
                    _ => return Err(SyncError::Corrupted(format!("未知的特殊编码类型: {}", encoding))),
                };
                Ok(Bytes::from(value.into_bytes()))
            }
            _ => unreachable!(),
        }
    }

    /// 读取原始字符串数据
    async fn read_string_raw(&mut self, length: usize) -> Result<Bytes> {
        if length == 0 {
            return Ok(Bytes::new());
        }

        let mut buffer = BytesMut::with_capacity(length);
        buffer.resize(length, 0);
        self.reader.read_exact(&mut buffer).await?;

        Ok(buffer.freeze())
    }
    
    /// 读取 AUX 字段
    async fn read_aux_field(&mut self) -> Result<RdbAuxField> {
        let key = self.read_string().await?;
        let value = self.read_string().await?;
        
        Ok(RdbAuxField::new(key, value))
    }
    
    /// 读取键值对
    async fn read_key_value(&mut self, rdb_type: RdbType) -> Result<BinEntry> {
        // 读取 key
        let key = self.read_string().await?;

        // 读取 value 和原始 RDB 编码数据
        let (value, raw_value) = self.read_value(rdb_type).await?;

        // 创建 BinEntry
        let entry = BinEntry::new(
            self.state.current_db,
            key,
            value,
            rdb_type,
        ).with_raw_value(raw_value);

        // 设置过期时间
        let entry = if let Some(expire) = self.state.current_expire {
            entry.with_expire(expire)
        } else {
            entry
        };

        // 清空当前过期时间
        self.state.current_expire = None;

        Ok(entry)
    }
    
    /// 读取值
    ///
    /// 根据 RDB 类型读取对应的值。
    /// 同时返回原始 RDB 编码数据（用于 DUMP 格式）。
    ///
    /// 重要：对于 String 类型，我们需要捕获原始字节（包括长度编码），
    /// 而不是解码后重新编码，因为原始数据可能使用了特殊的整数编码。
    async fn read_value(&mut self, rdb_type: RdbType) -> Result<(Bytes, Bytes)> {
        match rdb_type {
            RdbType::String => {
                // 读取字符串的原始字节（包括长度编码）
                let (value, raw_bytes) = self.read_string_with_raw_bytes().await?;

                // raw_value 不包含类型字节，只包含长度编码和数据
                // DUMP 格式: [version][length_encoded_data][crc64]
                Ok((value, raw_bytes))
            }

            // 其他类型的简化实现
            _ => {
                // 对于非 String 类型，捕获原始字节
                // 注意：不包含类型字节，因为 DUMP 格式不需要
                let mut raw_data = BytesMut::new();

                // 读取长度和数据
                match self.read_length().await {
                    Ok(length) if length > 0 && length < 100_000_000 => {
                        // 添加长度编码
                        self.encode_length_to_buf(length, &mut raw_data);

                        // 读取数据
                        let mut buffer = vec![0u8; length];
                        if let Err(e) = self.reader.read_exact(&mut buffer).await {
                            tracing::warn!("读取 RDB value 失败: {}, 类型: {:?}", e, rdb_type);
                            return Ok((Bytes::new(), Bytes::new()));
                        }
                        raw_data.extend_from_slice(&buffer);

                        Ok((Bytes::from(buffer), raw_data.freeze()))
                    }
                    Ok(_) => {
                        // 长度无效，返回空
                        Ok((Bytes::new(), Bytes::new()))
                    }
                    Err(e) => {
                        tracing::warn!("读取 RDB value 长度失败: {}, 类型: {:?}", e, rdb_type);
                        Ok((Bytes::new(), Bytes::new()))
                    }
                }
            }
        }
    }

    /// 读取字符串并返回解码后的值和原始字节
    ///
    /// 返回 (decoded_value, raw_bytes)
    /// raw_bytes 包含长度编码和数据，用于构建 DUMP 格式
    async fn read_string_with_raw_bytes(&mut self) -> Result<(Bytes, Bytes)> {
        // 读取第一个字节以确定编码方式
        let first_byte = self.read_byte().await?;
        let length_type = first_byte >> 6;

        match length_type {
            0 => { // 6 bit length
                let len = first_byte as usize & 0x3F;
                let data = self.read_string_raw(len).await?;
                let mut raw_bytes = BytesMut::new();
                raw_bytes.extend_from_slice(&[first_byte]);
                raw_bytes.extend_from_slice(&data);
                Ok((data, raw_bytes.freeze()))
            }
            1 => { // 14 bit length
                let second_byte = self.read_byte().await?;
                let len = ((first_byte as usize & 0x3F) << 8) | second_byte as usize;
                let data = self.read_string_raw(len).await?;
                let mut raw_bytes = BytesMut::new();
                raw_bytes.extend_from_slice(&[first_byte, second_byte]);
                raw_bytes.extend_from_slice(&data);
                Ok((data, raw_bytes.freeze()))
            }
            2 => { // 32 bit length
                let len = self.read_u32().await? as usize;
                let data = self.read_string_raw(len).await?;
                let mut raw_bytes = BytesMut::new();
                raw_bytes.extend_from_slice(&[first_byte]);
                raw_bytes.extend_from_slice(&(len as u32).to_le_bytes());
                raw_bytes.extend_from_slice(&data);
                Ok((data, raw_bytes.freeze()))
            }
            3 => { // Special encoding - integer as string
                let encoding = first_byte & 0x3F;
                let (value, raw_bytes) = match encoding {
                    0 => { // 8 bit integer
                        let b = self.read_byte().await?;
                        let value = Bytes::from(format!("{}", b as i8).into_bytes());
                        let mut raw = BytesMut::new();
                        raw.extend_from_slice(&[first_byte, b]);
                        (value, raw.freeze())
                    }
                    1 => { // 16 bit integer
                        let v = self.read_u16().await?;
                        let value = Bytes::from(format!("{}", v as i16).into_bytes());
                        let mut raw = BytesMut::new();
                        raw.extend_from_slice(&[first_byte]);
                        raw.extend_from_slice(&v.to_le_bytes());
                        (value, raw.freeze())
                    }
                    2 => { // 32 bit integer
                        let v = self.read_u32().await?;
                        let value = Bytes::from(format!("{}", v as i32).into_bytes());
                        let mut raw = BytesMut::new();
                        raw.extend_from_slice(&[first_byte]);
                        raw.extend_from_slice(&v.to_le_bytes());
                        (value, raw.freeze())
                    }
                    _ => return Err(SyncError::Corrupted(format!("未知的特殊编码类型: {}", encoding))),
                };
                Ok((value, raw_bytes))
            }
            _ => unreachable!(),
        }
    }

    /// 将字符串编码为 RDB 格式
    fn encode_string_as_rdb(&self, value: &Bytes) -> Bytes {
        let mut buf = BytesMut::new();

        // 添加 String 类型字节
        buf.extend_from_slice(&[RdbType::String as u8]);

        // 添加长度编码
        let len = value.len();
        self.encode_length_to_buf(len, &mut buf);

        // 添加数据
        buf.extend_from_slice(value);

        buf.freeze()
    }

    /// 将长度编码写入缓冲区
    fn encode_length_to_buf(&self, length: usize, buf: &mut BytesMut) {
        if length < 64 {
            // 6 bit length
            buf.extend_from_slice(&[length as u8]);
        } else if length < 16384 {
            // 14 bit length
            buf.extend_from_slice(&[
                ((length >> 8) as u8) | 0x40,
                (length & 0xFF) as u8,
            ]);
        } else {
            // 32 bit length
            buf.extend_from_slice(&[0x80]);
            buf.extend_from_slice(&(length as u32).to_le_bytes());
        }
    }
    
    /// 读取 u32
    async fn read_u32(&mut self) -> Result<u32> {
        let value = self.reader.read_u32_le().await?;
        Ok(value)
    }
    
    /// 读取 u64
    async fn read_u64(&mut self) -> Result<u64> {
        let value = self.reader.read_u64_le().await?;
        Ok(value)
    }
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    use tempfile::tempdir;
    
    /// 测试 RDB 解析器基础功能
    #[tokio::test]
    async fn test_rdb_parser_magic() {
        // 创建测试数据：REDIS + 版本 + EOF
        let mut data = Vec::new();
        data.extend_from_slice(b"REDIS0006");  // Magic + Version
        data.push(0xFF);  // EOF
        
        let cursor = std::io::Cursor::new(data);
        let mut parser = RdbParser::new(cursor);
        
        let result = parser.parse(|_| Ok(())).await;
        assert!(result.is_ok());
        
        assert!(parser.state.eof_reached);
        assert_eq!(parser.state.version, 6);
    }
    
    /// 测试 RDB 解析器无效魔数
    #[tokio::test]
    async fn test_rdb_parser_invalid_magic() {
        let mut data = Vec::new();
        data.extend_from_slice(b"XXXXX0006");  // 无效魔数
        data.push(0xFF);
        
        let cursor = std::io::Cursor::new(data);
        let mut parser = RdbParser::new(cursor);
        
        let result = parser.parse(|_| Ok(())).await;
        assert!(result.is_err());
    }
}