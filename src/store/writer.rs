//! store/writer.rs - 数据写入器实现
//!
//! 本文件实现 RdbWriter 和 AofWriter，支持异步写入 RDB 和 AOF 数据。

use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncWrite, AsyncWriteExt, BufWriter};
use tokio::fs::File;
use tokio::sync::Notify;
use crate::config::LocalCacheConfig;

/// RDB 文件写入器
///
/// 使用 BufWriter 缓冲写入，提升性能。
pub struct RdbWriter {
    /// 缓冲写入器
    writer: BufWriter<File>,
    /// 文件路径（用于日志）
    path: PathBuf,
    /// 已写入字节数
    written: u64,
    /// 数据通知器
    notify: Arc<Notify>,
}

impl RdbWriter {
    /// 创建 RDB Writer
    ///
    /// # 参数
    /// - writer: BufWriter<File>
    /// - path: 文件路径
    /// - notify: 数据通知器
    pub fn new(writer: BufWriter<File>, path: PathBuf, notify: Arc<Notify>) -> Self {
        RdbWriter {
            writer,
            path,
            written: 0,
            notify,
        }
    }
}

impl AsyncWrite for RdbWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();

        let result = Pin::new(&mut this.writer).poll_write(cx, buf);

        match result {
            Poll::Ready(Ok(n)) => {
                this.written += n as u64;
                // 通知等待者有新数据
                this.notify.notify_waiters();
                Poll::Ready(Ok(n))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
    
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().writer).poll_flush(cx)
    }
    
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().writer).poll_shutdown(cx)
    }
}

/// AOF 文件写入器
///
/// 支持文件轮转，当文件大小超过配置的 log_size 时自动创建新文件。
pub struct AofWriter {
    /// 缓冲写入器
    writer: BufWriter<File>,
    /// 当前文件路径
    current_path: PathBuf,
    /// 已写入字节数
    written: u64,
    /// 配置
    config: LocalCacheConfig,
    /// runId 目录（用于创建新文件）
    run_id_dir: PathBuf,
    /// 当前 offset
    current_offset: i64,
    /// 数据通知器
    notify: Arc<Notify>,
}

impl AofWriter {
    /// 创建 AOF Writer
    ///
    /// # 参数
    /// - writer: BufWriter<File>
    /// - path: 当前文件路径
    /// - config: 存储配置
    /// - notify: 数据通知器
    pub fn new(writer: BufWriter<File>, path: PathBuf, config: LocalCacheConfig, notify: Arc<Notify>) -> Self {
        let run_id_dir = path.parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(""));

        // 从文件名解析 offset
        let current_offset = path.file_name()
            .and_then(|n| n.to_str())
            .and_then(|name| {
                if name.ends_with(".aof") {
                    name.trim_end_matches(".aof").parse::<i64>().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);

        AofWriter {
            writer,
            current_path: path,
            written: 0,
            config,
            run_id_dir,
            current_offset,
            notify,
        }
    }
    
    /// 检查是否需要轮转文件
    async fn check_rotation(&mut self) -> std::io::Result<()> {
        if self.written >= self.config.log_size as u64 {
            // 刷新当前文件
            self.writer.flush().await?;
            
            // 创建新文件
            let new_offset = self.current_offset + self.written as i64;
            let new_path = self.run_id_dir.join(format!("{}.aof", new_offset));
            
            let new_file = tokio::fs::File::create(&new_path).await?;
            self.writer = BufWriter::new(new_file);
            self.current_path = new_path;
            self.current_offset = new_offset;
            self.written = 0;
            
            tracing::info!("AOF 文件轮转: {}", self.current_path.display());
        }
        
        Ok(())
    }
}

impl AsyncWrite for AofWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();

        let result = Pin::new(&mut this.writer).poll_write(cx, buf);

        match result {
            Poll::Ready(Ok(n)) => {
                this.written += n as u64;
                Poll::Ready(Ok(n))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();

        // 先刷新写入器
        let result = Pin::new(&mut this.writer).poll_flush(cx);

        match result {
            Poll::Ready(Ok(())) => {
                // flush 成功后通知等待者有新数据
                this.notify.notify_waiters();
                Poll::Ready(Ok(()))
            }
            _ => result,
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().writer).poll_shutdown(cx)
    }
}

/// 扩展方法：手动检查轮转
impl AofWriter {
    /// 检查并执行轮转（异步方法）
    pub async fn rotate_if_needed(&mut self) -> std::io::Result<()> {
        self.check_rotation().await
    }
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    /// 测试 RdbWriter 写入
    #[tokio::test]
    async fn test_rdb_writer() {
        let temp_dir = tempdir().unwrap();
        let rdb_path = temp_dir.path().join("test.rdb");

        let file = tokio::fs::File::create(&rdb_path).await.unwrap();
        let writer = BufWriter::new(file);
        let notify = Arc::new(Notify::new());
        let mut rdb_writer = RdbWriter::new(writer, rdb_path.clone(), notify);

        // 写入数据
        rdb_writer.write_all(b"test rdb data").await.unwrap();
        rdb_writer.flush().await.unwrap();

        // 验证文件存在
        assert!(tokio::fs::try_exists(&rdb_path).await.unwrap());

        // 验证内容
        let content = tokio::fs::read(&rdb_path).await.unwrap();
        assert_eq!(content, b"test rdb data");
    }

    /// 测试 AofWriter 写入
    #[tokio::test]
    async fn test_aof_writer() {
        let temp_dir = tempdir().unwrap();
        let aof_path = temp_dir.path().join("100.aof");

        let file = tokio::fs::File::create(&aof_path).await.unwrap();
        let writer = BufWriter::new(file);
        let config = LocalCacheConfig::default();
        let notify = Arc::new(Notify::new());
        let mut aof_writer = AofWriter::new(writer, aof_path.clone(), config, notify);
        
        // 写入 RESP 命令
        let cmd = b"*3\r\n$3\r\nSET\r\n$3\r\nkey\r\n$3\r\nval\r\n";
        aof_writer.write_all(cmd).await.unwrap();
        aof_writer.flush().await.unwrap();
        
        // 验证文件存在
        assert!(tokio::fs::try_exists(&aof_path).await.unwrap());
        
        // 验证内容
        let content = tokio::fs::read(&aof_path).await.unwrap();
        assert_eq!(content, cmd);
    }
    
    /// 测试 AofWriter 轮转（需要手动调用）
    #[tokio::test]
    async fn test_aof_writer_rotation() {
        let temp_dir = tempdir().unwrap();
        let run_dir = temp_dir.path().join("run123");
        tokio::fs::create_dir(&run_dir).await.unwrap();

        let aof_path = run_dir.join("0.aof");

        let file = tokio::fs::File::create(&aof_path).await.unwrap();
        let writer = BufWriter::new(file);

        // 设置小的 log_size 以触发轮转
        let config = LocalCacheConfig {
            log_size: 10, // 10 字节
            ..Default::default()
        };

        let notify = Arc::new(Notify::new());
        let mut aof_writer = AofWriter::new(writer, aof_path.clone(), config, notify);
        
        // 写入超过 log_size 的数据
        aof_writer.write_all(b"0123456789ABCDEF").await.unwrap();
        aof_writer.flush().await.unwrap();
        
        // 手动检查轮转
        aof_writer.rotate_if_needed().await.unwrap();
        
        // 验证原文件存在
        assert!(tokio::fs::try_exists(&aof_path).await.unwrap());
        
        // 验证新文件存在（offset=16）
        let new_path = run_dir.join("16.aof");
        assert!(tokio::fs::try_exists(&new_path).await.unwrap());
    }
}