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
    
    /// 获取解析出的 RDB 版本号
    pub fn rdb_version(&self) -> u32 {
        self.state.version
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
                0xF7 => { // ModuleAux
                    // 跳过 ModuleAux 数据
                    // ModuleAux 结构: module_id (length encoded) + module data (complex structure)
                    // 参考 Go 代码 rdbLoadCheckModuleValue
                    let _module_id = self.read_length().await?;
                    // 跳过模块数据
                    if let Err(e) = self.skip_module_value().await {
                        tracing::warn!("跳过 ModuleAux 数据失败: {}", e);
                        return Err(e);
                    }
                    tracing::debug!("跳过 ModuleAux 数据");
                }

                0xF8 => { // Idle
                    let _idle = self.read_length().await?;
                    // Idle time 是键的空辅助信息，不影响数据解析
                    tracing::debug!("跳过 Idle 数据");
                }

                0xF9 => { // Freq
                    let _freq = self.read_byte().await?;
                    // Freq 是键的辅助信息，不影响数据解析
                    tracing::debug!("跳过 Freq 数据");
                }

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
                        // 对于 Module 类型，读取并跳过，不生成 BinEntry
                        if matches!(rdb_type, RdbType::Module | RdbType::Module2) {
                            let _entry = self.read_key_value(rdb_type).await?;
                            // Module 数据暂不支持，直接跳过
                            tracing::debug!("跳过 Module 类型数据");
                        } else {
                            let entry = self.read_key_value(rdb_type).await?;
                            callback(entry)?;
                        }
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
            2 => { // 32 bit length 或特殊编码
                // 检查是否是 rdb32bitLen (0x80) 或 rdb64bitLen (0x81)
                match first_byte {
                    0x80 => { // rdb32bitLen: 32 bit length (big endian)
                        let length = self.read_u32_be().await?;
                        Ok(length as usize)
                    }
                    0x81 => { // rdb64bitLen: 64 bit length (big endian)
                        let length = self.read_u64_be().await?;
                        Ok(length as usize)
                    }
                    _ => { // 特殊编码 - 这不是一个纯长度值
                        Err(SyncError::Corrupted(format!("Special encoding encountered: {}", first_byte)))
                    }
                }
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
    ///
    /// 编码格式（参考 Redis rdb.c rdbLoadLen）：
    /// - 前 2 bit = 00: 6 bit length
    /// - 前 2 bit = 01: 14 bit length
    /// - 前 2 bit = 10: 特殊处理
    ///   - 整个字节 = 0x80 (rdb32bitLen): 32 bit length (big endian)
    ///   - 整个字节 = 0x81 (rdb64bitLen): 64 bit length (big endian)
    ///   - 其他: 特殊编码（前 2 bit = 11），后 6 bit 为编码类型
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
            2 => { // 32 bit length 或特殊编码
                // 检查是否是 rdb32bitLen (0x80) 或 rdb64bitLen (0x81)
                match first_byte {
                    0x80 => { // rdb32bitLen: 32 bit length (big endian)
                        let length = self.read_u32_be().await?;
                        Ok((length as usize, false, 0))
                    }
                    0x81 => { // rdb64bitLen: 64 bit length (big endian)
                        let length = self.read_u64_be().await?;
                        Ok((length as usize, false, 0))
                    }
                    _ => { // 特殊编码 (前 2 bit = 11)
                        let encoding = first_byte & 0x3F;
                        let value = match encoding {
                            0 => self.read_byte().await? as u64, // 8 bit integer
                            1 => self.read_u16().await? as u64,  // 16 bit integer
                            2 => self.read_u32().await? as u64,  // 32 bit integer (little endian)
                            _ => return Err(SyncError::Corrupted(format!("未知的特殊编码类型: {}", encoding))),
                        };
                        Ok((0, true, value))
                    }
                }
            }
            3 => { // Special encoding (前 2 bit = 11)
                let encoding = first_byte & 0x3F;
                let value = match encoding {
                    0 => self.read_byte().await? as u64, // 8 bit integer
                    1 => self.read_u16().await? as u64,  // 16 bit integer
                    2 => self.read_u32().await? as u64,  // 32 bit integer (little endian)
                    _ => return Err(SyncError::Corrupted(format!("未知的特殊编码类型: {}", encoding))),
                };
                Ok((0, true, value))
            }
            _ => unreachable!(),
        }
    }

    /// 读取长度编码值，同时返回原始编码字节
    ///
    /// 返回 (长度值, 原始编码字节)
    /// 原始字节可直接用于构建 RESTORE payload。
    async fn read_length_with_raw(&mut self) -> Result<(usize, Bytes)> {
        let first_byte = self.read_byte().await?;
        let length_type = first_byte >> 6;
        let mut raw = BytesMut::new();
        raw.extend_from_slice(&[first_byte]);

        match length_type {
            0 => { // 6 bit length
                Ok((first_byte as usize & 0x3F, raw.freeze()))
            }
            1 => { // 14 bit length
                let second_byte = self.read_byte().await?;
                raw.extend_from_slice(&[second_byte]);
                let len = ((first_byte as usize & 0x3F) << 8) | second_byte as usize;
                Ok((len, raw.freeze()))
            }
            2 => { // 32/64 bit length
                match first_byte {
                    0x80 => { // rdb32bitLen
                        let len = self.read_u32_be().await?;
                        raw.extend_from_slice(&len.to_be_bytes());
                        Ok((len as usize, raw.freeze()))
                    }
                    0x81 => { // rdb64bitLen
                        let len = self.read_u64_be().await?;
                        raw.extend_from_slice(&len.to_be_bytes());
                        Ok((len as usize, raw.freeze()))
                    }
                    _ => Err(SyncError::Corrupted(format!(
                        "read_length_with_raw: 非法的长度编码字节: {:#x}",
                        first_byte
                    ))),
                }
            }
            3 => { // Special encoding: 8/16/32 bit integer
                let encoding = first_byte & 0x3F;
                match encoding {
                    0 => {
                        let b = self.read_byte().await?;
                        raw.extend_from_slice(&[b]);
                        Ok((b as usize, raw.freeze()))
                    }
                    1 => {
                        let v = self.read_u16().await?;
                        raw.extend_from_slice(&v.to_le_bytes());
                        Ok((v as usize, raw.freeze()))
                    }
                    2 => {
                        let v = self.read_u32().await?;
                        raw.extend_from_slice(&v.to_le_bytes());
                        Ok((v as usize, raw.freeze()))
                    }
                    _ => Err(SyncError::Corrupted(format!(
                        "read_length_with_raw: 未知的特殊编码类型: {}",
                        encoding
                    ))),
                }
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
            2 => { // 32 bit 或 64 bit 长度 (big endian)
                match first_byte {
                    0x80 => { // rdb32bitLen: 32 bit big endian
                        let len = self.read_u32_be().await? as usize;
                        self.read_string_raw(len).await
                    }
                    0x81 => { // rdb64bitLen: 64 bit big endian
                        let len = self.read_u64_be().await? as usize;
                        self.read_string_raw(len).await
                    }
                    _ => Err(SyncError::Corrupted(format!(
                        "read_string: 非法的长度编码字节: {:#x}",
                        first_byte
                    ))),
                }
            }
            3 => { // Special encoding - integer as string or compressed data
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
                    3 => { // LZF compressed string - 读取压缩长度和原始长度，然后跳过压缩数据
                        let compressed_len = self.read_length().await?;
                        let original_len = self.read_length().await?;
                        // 跳过压缩数据
                        let mut compressed_data = vec![0u8; compressed_len];
                        self.reader.read_exact(&mut compressed_data).await?;
                        tracing::debug!("遇到 LZF 压缩字符串，跳过解压保留原始字节 (compressed_len={}, original_len={})", 
                            compressed_len, original_len);
                        // 返回空字符串作为占位
                        String::new()
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
    /// 同时返回原始 RDB 编码数据（用于 RESTORE/DUMP 格式）。
    ///
    /// 设计原则：不深入解析 value 内部结构，
    /// 而是按类型提取出 value 的原始二进制块，
    /// 因为数据会通过 RESTORE 命令原样回放。
    async fn read_value(&mut self, rdb_type: RdbType) -> Result<(Bytes, Bytes)> {
        match rdb_type {
            RdbType::String => {
                let (value, raw_bytes) = self.read_string_with_raw_bytes().await?;
                Ok((value, raw_bytes))
            }

            // ===== 压缩类型：value 是单个字符串 (可能 LZF 压缩) =====
            RdbType::HashZipmap
            | RdbType::ListZiplist
            | RdbType::SetIntset
            | RdbType::SortedSetZiplist
            | RdbType::HashZiplist
            | RdbType::HashListpack
            | RdbType::SortedSetListpack
            | RdbType::SetListpack => {
                let (value, raw_bytes) = self.read_string_with_raw_bytes().await?;
                Ok((value, raw_bytes))
            }

            // ===== 旧格式 List / Set：[len] + N × [string] =====
            RdbType::List | RdbType::Set => {
                self.read_list_set_raw_value().await
            }

            // ===== 旧格式 Hash：[len] + N × ([field_string][value_string]) =====
            RdbType::Hash => {
                self.read_hash_raw_value().await
            }

            // ===== 旧格式 ZSet：[len] + N × ([member_string][score_double_8B_LE]) =====
            RdbType::SortedSet | RdbType::SortedSet2 => {
                self.read_zset_raw_value().await
            }

            // ===== Quicklist：[len] + N × [ziplist/listpack_string] =====
            RdbType::Quicklist | RdbType::Quicklist2 => {
                self.read_quicklist_raw_value().await
            }

            // Module 类型：跳过
            RdbType::Module | RdbType::Module2 => {
                let _module_id = self.read_u64().await?;
                let _module_data = self.read_string().await?;
                tracing::debug!("跳过 Module 数据: type={:?}, module_id={}", rdb_type, _module_id);
                Ok((Bytes::new(), Bytes::new()))
            }

            // Stream 类型：已有专门处理
            RdbType::StreamListpacks | RdbType::StreamListpacks2 | RdbType::StreamListpacks3 => {
                self.read_stream_value(rdb_type).await
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
            2 => { // 32 bit 或 64 bit 长度 (big endian)
                match first_byte {
                    0x80 => { // rdb32bitLen: 32 bit length (big endian)
                        let len = self.read_u32_be().await? as usize;
                        let data = self.read_string_raw(len).await?;
                        let mut raw_bytes = BytesMut::new();
                        raw_bytes.extend_from_slice(&[first_byte]);
                        raw_bytes.extend_from_slice(&(len as u32).to_be_bytes());
                        raw_bytes.extend_from_slice(&data);
                        Ok((data, raw_bytes.freeze()))
                    }
                    0x81 => { // rdb64bitLen: 64 bit length (big endian)
                        let len = self.read_u64_be().await? as usize;
                        let data = self.read_string_raw(len).await?;
                        let mut raw_bytes = BytesMut::new();
                        raw_bytes.extend_from_slice(&[first_byte]);
                        raw_bytes.extend_from_slice(&(len as u64).to_be_bytes());
                        raw_bytes.extend_from_slice(&data);
                        Ok((data, raw_bytes.freeze()))
                    }
                    _ => Err(SyncError::Corrupted(format!(
                        "read_string_with_raw_bytes: 非法的长度编码字节: {:#x}",
                        first_byte
                    ))),
                }
            }
            3 => { // Special encoding - integer as string or compressed data
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
                    3 => { // LZF compressed string
                        let compressed_len = self.read_length().await?;
                        let original_len = self.read_length().await?;
                        let mut compressed_data = vec![0u8; compressed_len];
                        self.reader.read_exact(&mut compressed_data).await?;
                        // tracing::debug!("遇到 LZF 压缩字符串，跳过解压保留原始字节 (compressed_len={}, original_len={})", 
                        //     compressed_len, original_len);
                        // 返回空值和原始字节（包含压缩数据）
                        let mut raw = BytesMut::new();
                        raw.extend_from_slice(&[first_byte]);
                        self.encode_length_to_buf(compressed_len, &mut raw);
                        self.encode_length_to_buf(original_len, &mut raw);
                        raw.extend_from_slice(&compressed_data);
                        (Bytes::new(), raw.freeze())
                    }
                    _ => return Err(SyncError::Corrupted(format!("未知的特殊编码类型: {}", encoding))),
                };
                Ok((value, raw_bytes))
            }
            _ => unreachable!(),
        }
    }

    // ============================================================
    // 按类型提取 value 原始字节的方法
    //
    // 这些方法不深入解析 value 内部结构，
    // 而是将 type_byte 之后的完整原始二进制块提取出来，
    // 用于构建 RESTORE 命令的 serialized-value。
    // ============================================================

    /// 读取旧格式 List / Set 的原始字节
    ///
    /// RDB 格式: [len(元素数)] [string_1] [string_2] ...
    async fn read_list_set_raw_value(&mut self) -> Result<(Bytes, Bytes)> {
        let (count, len_raw) = self.read_length_with_raw().await?;

        // 安全上限：防止损坏数据导致 OOM
        if count > 100_000_000 {
            return Err(SyncError::Corrupted(format!(
                "List/Set 元素数量异常: {}",
                count
            )));
        }

        let mut raw = BytesMut::new();
        raw.extend_from_slice(&len_raw);

        for _ in 0..count {
            let (_elem_value, elem_raw) = self.read_string_with_raw_bytes().await?;
            raw.extend_from_slice(&elem_raw);
        }

        Ok((Bytes::new(), raw.freeze()))
    }

    /// 读取旧格式 Hash 的原始字节
    ///
    /// RDB 格式: [len(元素数)] [field_1][value_1] [field_2][value_2] ...
    async fn read_hash_raw_value(&mut self) -> Result<(Bytes, Bytes)> {
        let (count, len_raw) = self.read_length_with_raw().await?;

        if count > 100_000_000 {
            return Err(SyncError::Corrupted(format!(
                "Hash 元素数量异常: {}",
                count
            )));
        }

        let mut raw = BytesMut::new();
        raw.extend_from_slice(&len_raw);

        for _ in 0..count {
            let (_field, field_raw) = self.read_string_with_raw_bytes().await?;
            raw.extend_from_slice(&field_raw);
            let (_value, value_raw) = self.read_string_with_raw_bytes().await?;
            raw.extend_from_slice(&value_raw);
        }

        Ok((Bytes::new(), raw.freeze()))
    }

    /// 读取旧格式 ZSet / ZSet2 的原始字节
    ///
    /// RDB 格式: [len(元素数)] [member_1][score_1(8B LE double)] [member_2][score_2] ...
    async fn read_zset_raw_value(&mut self) -> Result<(Bytes, Bytes)> {
        let (count, len_raw) = self.read_length_with_raw().await?;

        if count > 100_000_000 {
            return Err(SyncError::Corrupted(format!(
                "ZSet 元素数量异常: {}",
                count
            )));
        }

        let mut raw = BytesMut::new();
        raw.extend_from_slice(&len_raw);

        for _ in 0..count {
            let (_member, member_raw) = self.read_string_with_raw_bytes().await?;
            raw.extend_from_slice(&member_raw);

            // score: 8 字节 little-endian double
            let mut score_buf = [0u8; 8];
            self.reader.read_exact(&mut score_buf).await?;
            raw.extend_from_slice(&score_buf);
        }

        Ok((Bytes::new(), raw.freeze()))
    }

    /// 读取 Quicklist / Quicklist2 的原始字节
    ///
    /// RDB 格式: [len(节点数)] [ziplist/listpack_string_1] [ziplist/listpack_string_2] ...
    async fn read_quicklist_raw_value(&mut self) -> Result<(Bytes, Bytes)> {
        let (count, len_raw) = self.read_length_with_raw().await?;

        if count > 100_000_000 {
            return Err(SyncError::Corrupted(format!(
                "Quicklist 节点数量异常: {}",
                count
            )));
        }

        let mut raw = BytesMut::new();
        raw.extend_from_slice(&len_raw);

        for _ in 0..count {
            let (_node, node_raw) = self.read_string_with_raw_bytes().await?;
            raw.extend_from_slice(&node_raw);
        }

        Ok((Bytes::new(), raw.freeze()))
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
    ///
    /// 参考 Redis rdb.c rdbSaveLen：
    /// - < 64: 1 byte (6 bit length)
    /// - < 16384: 2 bytes (14 bit length, big endian)
    /// - >= 16384: 5 bytes (0x80 + 4 bytes big endian)
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
            // 32 bit length (big endian)
            buf.extend_from_slice(&[0x80]);
            buf.extend_from_slice(&(length as u32).to_be_bytes());
        }
    }
    
    /// 读取 u32 (little endian)
    async fn read_u32(&mut self) -> Result<u32> {
        let value = self.reader.read_u32_le().await?;
        Ok(value)
    }

    /// 读取 u32 (big endian) - 用于 RDB 长度编码
    async fn read_u32_be(&mut self) -> Result<u32> {
        let value = self.reader.read_u32().await?;
        Ok(value)
    }

    /// 读取 u64 (little endian)
    async fn read_u64(&mut self) -> Result<u64> {
        let value = self.reader.read_u64_le().await?;
        Ok(value)
    }

    /// 读取 u64 (big endian) - 用于 RDB 长度编码
    async fn read_u64_be(&mut self) -> Result<u64> {
        let value = self.reader.read_u64().await?;
        Ok(value)
    }

    /// 跳过 Module 数据
    ///
    /// Module 数据结构（参考 Redis rdb.c rdbLoadCheckModuleValue）：
    /// - opcode (length encoded)
    /// - 如果 opcode == 0 (EOF)，停止
    /// - 如果 opcode == 1 (SINT) 或 2 (UINT)，读取另一个 length（值）
    /// - 如果 opcode == 3 (FLOAT)，读取 4 字节
    /// - 如果 opcode == 4 (DOUBLE)，读取 8 字节
    /// - 如果 opcode == 5 (STRING)，读取字符串
    /// - 重复
    async fn skip_module_value(&mut self) -> Result<()> {
        loop {
            let opcode = match self.read_length().await {
                Ok(op) => op,
                Err(e) => {
                    tracing::warn!("读取 module opcode 失败: {}", e);
                    return Err(e);
                }
            };

            if opcode == 0 {
                // EOF
                break;
            }

            match opcode {
                1 | 2 => {
                    // SINT or UINT
                    if let Err(e) = self.read_length().await {
                        tracing::warn!("读取 module SINT/UINT 值失败: {}", e);
                        return Err(e);
                    }
                }
                3 => {
                    // FLOAT (4 bytes)
                    let mut buf = [0u8; 4];
                    if let Err(e) = self.reader.read_exact(&mut buf).await {
                        tracing::warn!("读取 module FLOAT 值失败: {}", e);
                        return Err(SyncError::Io(e));
                    }
                }
                4 => {
                    // DOUBLE (8 bytes)
                    let mut buf = [0u8; 8];
                    if let Err(e) = self.reader.read_exact(&mut buf).await {
                        tracing::warn!("读取 module DOUBLE 值失败: {}", e);
                        return Err(SyncError::Io(e));
                    }
                }
                5 => {
                    // STRING
                    if let Err(e) = self.read_string().await {
                        tracing::warn!("读取 module STRING 值失败: {}", e);
                        return Err(e);
                    }
                }
                _ => {
                    tracing::warn!("未知的 module opcode: {}", opcode);
                    return Err(SyncError::Corrupted(format!("未知的 module opcode: {}", opcode)));
                }
            }
        }
        Ok(())
    }

    /// 读取 Stream 类型的值
    ///
    /// Stream 类型（StreamListpacks、StreamListpacks2 和 StreamListpacks3）的结构比较复杂：
    /// 参考 Redis 源码 rdb.c 中的 rdbSaveStream 和 rdbLoadStream 函数
    ///
    /// 由于 Stream 结构复杂且版本间有差异，这里采用更保守的策略：
    /// 读取并验证已知字段，遇到错误时优雅地跳过剩余数据
    async fn read_stream_value(&mut self, rdb_type: RdbType) -> Result<(Bytes, Bytes)> {
        // 读取 listpack 数量
        let listpack_count = match self.read_length_with_encoding().await {
            Ok((len, _, _)) => len,
            Err(e) => {
                tracing::warn!("读取 Stream listpack 数量失败: {}, 类型: {:?}", e, rdb_type);
                return Ok((Bytes::new(), Bytes::new()));
            }
        };

        // 跳过所有 listpack
        // 参考 Go 代码：每个 listpack 是一个 raw string (16 byte key + listpack data)
        for i in 0..listpack_count {
            // 读取 listpack 的 stream ID (16 bytes raw string)
            // 参考 Go 代码: key := r.ReadStringP()，然后检查 len(key) == 16
            let stream_id = match self.read_string().await {
                Ok(data) => data,
                Err(e) => {
                    tracing::warn!("读取 Stream listpack {} stream ID 失败: {}, 类型: {:?}", i, e, rdb_type);
                    return Ok((Bytes::new(), Bytes::new()));
                }
            };

            if stream_id.len() != 16 {
                tracing::warn!("Stream listpack {} stream ID 长度不是 16 字节: {}, 类型: {:?}", i, stream_id.len(), rdb_type);
                return Ok((Bytes::new(), Bytes::new()));
            }

            // 读取 listpack 数据 (raw string)
            if let Err(e) = self.read_string().await {
                tracing::warn!("读取 Stream listpack {} 数据失败: {}, 类型: {:?}", i, e, rdb_type);
                return Ok((Bytes::new(), Bytes::new()));
            }
        }

        // 读取 Stream ID (last_id: ms + seq)
        if let Err(e) = self.read_length_with_encoding().await {
            tracing::warn!("读取 Stream last_id_ms 失败: {}, 类型: {:?}", e, rdb_type);
            return Ok((Bytes::new(), Bytes::new()));
        }
        if let Err(e) = self.read_length_with_encoding().await {
            tracing::warn!("读取 Stream last_id_seq 失败: {}, 类型: {:?}", e, rdb_type);
            return Ok((Bytes::new(), Bytes::new()));
        }

        // StreamListpacks2 和 StreamListpacks3 有额外的元数据
        if rdb_type == RdbType::StreamListpacks2 || rdb_type == RdbType::StreamListpacks3 {
            // first_id (ms, seq)
            if let Err(e) = self.read_length_with_encoding().await {
                tracing::warn!("读取 Stream first_id_ms 失败: {}, 类型: {:?}", e, rdb_type);
                return Ok((Bytes::new(), Bytes::new()));
            }
            if let Err(e) = self.read_length_with_encoding().await {
                tracing::warn!("读取 Stream first_id_seq 失败: {}, 类型: {:?}", e, rdb_type);
                return Ok((Bytes::new(), Bytes::new()));
            }

            // max_deleted_entry_id (ms, seq)
            if let Err(e) = self.read_length_with_encoding().await {
                tracing::warn!("读取 Stream max_deleted_ms 失败: {}, 类型: {:?}", e, rdb_type);
                return Ok((Bytes::new(), Bytes::new()));
            }
            if let Err(e) = self.read_length_with_encoding().await {
                tracing::warn!("读取 Stream max_deleted_seq 失败: {}, 类型: {:?}", e, rdb_type);
                return Ok((Bytes::new(), Bytes::new()));
            }

            // entries_added
            if let Err(e) = self.read_length_with_encoding().await {
                tracing::warn!("读取 Stream entries_added 失败: {}, 类型: {:?}", e, rdb_type);
                return Ok((Bytes::new(), Bytes::new()));
            }
        }

        // 读取消费组数量
        let cgroups_count = match self.read_length_with_encoding().await {
            Ok((len, _, _)) => len,
            Err(e) => {
                tracing::warn!("读取 Stream 消费组数量失败: {}, 类型: {:?}", e, rdb_type);
                return Ok((Bytes::new(), Bytes::new()));
            }
        };

        // 跳过消费组数据
        for cg_idx in 0..cgroups_count {
            // 消费组名称（字符串）
            if let Err(e) = self.read_string().await {
                tracing::warn!("读取 Stream 消费组 {} 名称失败: {}, 类型: {:?}", cg_idx, e, rdb_type);
                return Ok((Bytes::new(), Bytes::new()));
            }

            // 消费组 last_id (ms, seq)
            if let Err(e) = self.read_length_with_encoding().await {
                tracing::warn!("读取 Stream 消费组 {} last_id_ms 失败: {}, 类型: {:?}", cg_idx, e, rdb_type);
                return Ok((Bytes::new(), Bytes::new()));
            }
            if let Err(e) = self.read_length_with_encoding().await {
                tracing::warn!("读取 Stream 消费组 {} last_id_seq 失败: {}, 类型: {:?}", cg_idx, e, rdb_type);
                return Ok((Bytes::new(), Bytes::new()));
            }

            // entries_read（仅 StreamListpacks2 和 StreamListpacks3）
            if rdb_type == RdbType::StreamListpacks2 || rdb_type == RdbType::StreamListpacks3 {
                if let Err(e) = self.read_length_with_encoding().await {
                    tracing::warn!("读取 Stream 消费组 {} entries_read 失败: {}, 类型: {:?}", cg_idx, e, rdb_type);
                    return Ok((Bytes::new(), Bytes::new()));
                }
            }

            // 读取全局 PEL 数量
            let global_pel_count = match self.read_length_with_encoding().await {
                Ok((len, _, _)) => len,
                Err(e) => {
                    tracing::warn!("读取 Stream 消费组 {} 全局 PEL 数量失败: {}, 类型: {:?}", cg_idx, e, rdb_type);
                    return Ok((Bytes::new(), Bytes::new()));
                }
            };

            // 跳过全局 PEL 条目
            // 参考 Go 代码：每个 PEL 条目是 16 bytes stream ID + 8 bytes delivery time + length encoded delivery count
            for pel_idx in 0..global_pel_count {
                // stream ID (16 bytes)
                let mut stream_id_buf = [0u8; 16];
                if let Err(e) = self.reader.read_exact(&mut stream_id_buf).await {
                    tracing::warn!("读取 Stream 全局 PEL 条目 {} stream ID 失败: {}, 类型: {:?}", pel_idx, e, rdb_type);
                    return Ok((Bytes::new(), Bytes::new()));
                }

                // delivery_time (8 bytes)
                if let Err(e) = self.read_u64().await {
                    tracing::warn!("读取 Stream 全局 PEL 条目 {} delivery_time 失败: {}, 类型: {:?}", pel_idx, e, rdb_type);
                    return Ok((Bytes::new(), Bytes::new()));
                }

                // delivery_count (length encoded)
                if let Err(e) = self.read_length_with_encoding().await {
                    tracing::warn!("读取 Stream 全局 PEL 条目 {} delivery_count 失败: {}, 类型: {:?}", pel_idx, e, rdb_type);
                    return Ok((Bytes::new(), Bytes::new()));
                }
            }

            // 读取消费者数量
            let consumers_count = match self.read_length_with_encoding().await {
                Ok((len, _, _)) => len,
                Err(e) => {
                    tracing::warn!("读取 Stream 消费组 {} 消费者数量失败: {}, 类型: {:?}", cg_idx, e, rdb_type);
                    return Ok((Bytes::new(), Bytes::new()));
                }
            };

            // 跳过消费者数据
            for cons_idx in 0..consumers_count {
                // 消费者名称（字符串）
                if let Err(e) = self.read_string().await {
                    tracing::warn!("读取 Stream 消费者 {} 名称失败: {}, 类型: {:?}", cons_idx, e, rdb_type);
                    return Ok((Bytes::new(), Bytes::new()));
                }

                // 消费者 seen_time（8 bytes）
                if let Err(e) = self.read_u64().await {
                    tracing::warn!("读取 Stream 消费者 {} seen_time 失败: {}, 类型: {:?}", cons_idx, e, rdb_type);
                    return Ok((Bytes::new(), Bytes::new()));
                }

                // active_time（仅 StreamListpacks2 和 StreamListpacks3）
                if rdb_type == RdbType::StreamListpacks2 || rdb_type == RdbType::StreamListpacks3 {
                    if let Err(e) = self.read_u64().await {
                        tracing::warn!("读取 Stream 消费者 {} active_time 失败: {}, 类型: {:?}", cons_idx, e, rdb_type);
                        return Ok((Bytes::new(), Bytes::new()));
                    }
                }

                // 读取消费者 PEL 数量
                let pel_count = match self.read_length_with_encoding().await {
                    Ok((len, _, _)) => len,
                    Err(e) => {
                        tracing::warn!("读取 Stream 消费者 {} PEL 数量失败: {}, 类型: {:?}", cons_idx, e, rdb_type);
                        return Ok((Bytes::new(), Bytes::new()));
                    }
                };

                // 跳过消费者 PEL 条目
                // 参考 Go 代码：每个 PEL 条目只有 16 bytes stream ID
                for pel_idx in 0..pel_count {
                    // stream ID (16 bytes)
                    let mut stream_id_buf = [0u8; 16];
                    if let Err(e) = self.reader.read_exact(&mut stream_id_buf).await {
                        tracing::warn!("读取 Stream 消费者 PEL 条目 {} stream ID 失败: {}, 类型: {:?}", pel_idx, e, rdb_type);
                        return Ok((Bytes::new(), Bytes::new()));
                    }
                }
            }
        }

        tracing::debug!("跳过 Stream 数据: type={:?}", rdb_type);

        // 返回空值，表示跳过此条目
        // 注意：由于 Stream 结构复杂，我们不尝试重建原始字节
        Ok((Bytes::new(), Bytes::new()))
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