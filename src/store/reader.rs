//! store/reader.rs - 数据读取器实现
//!
//! 本文件实现 RdbReader 和 AofReader，支持从文件读取 RDB 和 AOF 数据。

use std::io::Seek;
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncSeek, AsyncSeekExt, ReadBuf};
use async_trait::async_trait;
use crate::error::{SyncError, Result};
use crate::store::{Reader, ReaderType};

/// RDB 文件读取器
///
/// 支持 seek 到指定位置读取 RDB 数据。
pub struct RdbReader {
    /// 文件句柄
    file: File,
    /// 数据起始偏移量
    offset: i64,
    /// 数据大小
    size: i64,
    /// 当前读取位置
    current_pos: i64,
}

impl RdbReader {
    /// 打开 RDB 文件
    ///
    /// # 参数
    /// - path: RDB 文件路径
    /// - offset: 数据起始偏移量
    /// - size: 数据大小
    pub fn open(path: &Path, offset: i64, size: i64) -> Result<Self> {
        // 使用 blocking_open，因为 tokio::fs::File::open 是异步的
        // 但我们需要在同步上下文中调用
        // 这里使用 std::fs::File，然后转换为 tokio::fs::File
        let std_file = std::fs::File::open(path)
            .map_err(|e| SyncError::Io(e))?;
        
        let file = File::from_std(std_file);
        
        Ok(RdbReader {
            file,
            offset,
            size,
            current_pos: offset,
        })
    }
    
    /// 异步打开 RDB 文件
    pub async fn open_async(path: &Path, offset: i64, size: i64) -> Result<Self> {
        let file = File::open(path).await?;
        
        Ok(RdbReader {
            file,
            offset,
            size,
            current_pos: offset,
        })
    }
}

impl AsyncRead for RdbReader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        
        // 计算剩余可读字节数
        let remaining = this.size - (this.current_pos - this.offset);
        if remaining <= 0 {
            return Poll::Ready(Ok(())); // EOF
        }
        
        // 限制读取大小不超过剩余数据
        let max_read = std::cmp::min(buf.remaining(), remaining as usize);
        let mut limited_buf = ReadBuf::new(buf.initialize_unfilled_to(max_read));
        
        // 读取文件
        let result = Pin::new(&mut this.file).poll_read(cx, &mut limited_buf);
        
        match result {
            Poll::Ready(Ok(())) => {
                let n = limited_buf.filled().len();
                this.current_pos += n as i64;
                buf.advance(n);
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncSeek for RdbReader {
    fn start_seek(
        self: Pin<&mut Self>,
        position: std::io::SeekFrom,
    ) -> std::io::Result<()> {
        let this = self.get_mut();
        
        // 计算实际 seek 位置（相对于 offset）
        let new_pos = match position {
            std::io::SeekFrom::Start(n) => this.offset + n as i64,
            std::io::SeekFrom::Current(n) => this.current_pos + n,
            std::io::SeekFrom::End(n) => this.offset + this.size + n,
        };
        
        // 检查边界
        if new_pos < this.offset || new_pos > this.offset + this.size {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Seek position out of bounds"
            ));
        }
        
        Pin::new(&mut this.file).start_seek(std::io::SeekFrom::Start(new_pos as u64))?;
        this.current_pos = new_pos;
        
        Ok(())
    }
    
    fn poll_complete(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<u64>> {
        Pin::new(&mut self.get_mut().file).poll_complete(cx)
    }
}

#[async_trait]
impl Reader for RdbReader {
    fn reader_type(&self) -> ReaderType {
        ReaderType::Rdb
    }
    
    fn offset(&self) -> i64 {
        self.offset
    }
    
    fn size(&self) -> Option<i64> {
        Some(self.size)
    }
}

/// AOF 文件读取器
///
/// 支持 seek 到指定位置读取 AOF 命令流。
pub struct AofReader {
    /// 文件句柄
    file: File,
    /// 数据起始偏移量
    offset: i64,
    /// 当前读取位置
    current_pos: i64,
}

impl AofReader {
    /// 打开 AOF 文件
    ///
    /// # 参数
    /// - path: AOF 文件路径
    /// - offset: 全局数据偏移量
    /// - aof_start_offset: AOF 文件的起始全局偏移量
    pub fn open(path: &Path, offset: i64, aof_start_offset: i64) -> Result<Self> {
        let mut std_file = std::fs::File::open(path)?;
        let file_pos = (offset - aof_start_offset) as u64;
        std_file.seek(std::io::SeekFrom::Start(file_pos))?;
        let file = File::from_std(std_file);
        
        Ok(AofReader {
            file,
            offset,
            current_pos: offset,
        })
    }
    
    /// 异步打开 AOF 文件
    pub async fn open_async(path: &Path, offset: i64, aof_start_offset: i64) -> Result<Self> {
        let mut file = File::open(path).await?;
        let file_pos = (offset - aof_start_offset) as u64;
        file.seek(std::io::SeekFrom::Start(file_pos)).await?;
        
        Ok(AofReader {
            file,
            offset,
            current_pos: offset,
        })
    }
}

impl AsyncRead for AofReader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        
        let result = Pin::new(&mut this.file).poll_read(cx, buf);
        
        match result {
            Poll::Ready(Ok(())) => {
                let n = buf.filled().len();
                this.current_pos += n as i64;
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncSeek for AofReader {
    fn start_seek(
        self: Pin<&mut Self>,
        position: std::io::SeekFrom,
    ) -> std::io::Result<()> {
        let this = self.get_mut();
        
        let new_pos = match position {
            std::io::SeekFrom::Start(n) => this.offset + n as i64,
            std::io::SeekFrom::Current(n) => this.current_pos + n,
            std::io::SeekFrom::End(_n) => {
                // 对于 AOF，end 是文件末尾
                // 需要先获取文件大小
                unimplemented!("AofReader SeekFrom::End not supported")
            }
        };
        
        Pin::new(&mut this.file).start_seek(std::io::SeekFrom::Start(new_pos as u64))?;
        this.current_pos = new_pos;
        
        Ok(())
    }
    
    fn poll_complete(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<u64>> {
        Pin::new(&mut self.get_mut().file).poll_complete(cx)
    }
}

#[async_trait]
impl Reader for AofReader {
    fn reader_type(&self) -> ReaderType {
        ReaderType::Aof
    }
    
    fn offset(&self) -> i64 {
        self.offset
    }
    
    fn size(&self) -> Option<i64> {
        None // AOF 文件大小未知
    }
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::io::AsyncWriteExt;
    
    /// 测试 RdbReader 读取
    #[tokio::test]
    async fn test_rdb_reader() {
        let temp_dir = tempdir().unwrap();
        let rdb_path = temp_dir.path().join("test.rdb");
        
        // 创建并写入测试数据
        let mut file = File::create(&rdb_path).await.unwrap();
        file.write_all(b"test rdb content").await.unwrap();
        file.flush().await.unwrap();
        
        // 打开读取
        let reader = RdbReader::open_async(&rdb_path, 0, 16).await.unwrap();
        
        assert_eq!(reader.reader_type(), ReaderType::Rdb);
        assert_eq!(reader.offset(), 0);
        assert_eq!(reader.size(), Some(16));
    }
    
    /// 测试 AofReader 读取
    #[tokio::test]
    async fn test_aof_reader() {
        let temp_dir = tempdir().unwrap();
        let aof_path = temp_dir.path().join("test.aof");
        
        // 创建并写入测试数据
        let mut file = File::create(&aof_path).await.unwrap();
        file.write_all(b"*3\r\n$3\r\nSET\r\n$3\r\nkey\r\n$3\r\nval\r\n").await.unwrap();
        file.flush().await.unwrap();
        
        // 打开读取
        let reader = AofReader::open_async(&aof_path, 100, 100).await.unwrap();
        
        assert_eq!(reader.reader_type(), ReaderType::Aof);
        assert_eq!(reader.offset(), 100);
        assert_eq!(reader.size(), None);
    }
}