//! main.rs - 应用入口
//!
//! 本文件是 redis-ha-tool 的主入口，负责：
//! - CLI 参数解析
//! - 配置加载
//! - 日志初始化
//! - HTTP/gRPC 服务启动
//! - 同步器启动

use clap::Parser;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::signal;

use redis_syncer::{
    checkpoint::CheckpointManager,
    config::SyncConfig,
    error::Result,
    syncer::{SyncerImpl, Syncer, RedisInput, RedisOutput, FileChannel},
    store::FileStorer,
    cmd::{AppState, build_router},
    utils::init_logging,
};

/// CLI 参数
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// 配置文件路径
    #[arg(short, long, value_name = "FILE")]
    config: PathBuf,
}

/// 主函数
#[tokio::main]
async fn main() -> Result<()> {
    // 解析 CLI 参数
    let args = Args::parse();

    // 加载配置
    let config = Arc::new(SyncConfig::from_file(&args.config)?);

    // 初始化日志（使用配置文件中的日志设置）
    init_logging(&config.log);

    tracing::info!("启动 Redis Syncer");
    tracing::info!("配置文件: {}", args.config.display());
    
    tracing::info!(
        "源 Redis: {:?}",
        config.input.redis.addresses
    );
    tracing::info!(
        "目标 Redis: {:?}",
        config.output.redis.addresses
    );
    
    // 创建同步器
    let syncer = Arc::new(SyncerImpl::new(config.clone()));
    
    let storer = Arc::new(FileStorer::new(
        std::path::PathBuf::from(&config.local_cache.dir),
        config.local_cache.clone(),
    ));
    
    let channel = Arc::new(FileChannel::new(storer));
    syncer.set_channel(channel.clone()).await;
    
    let run_id = format!("run_{}", chrono::Utc::now().timestamp());
    syncer.set_run_id(run_id.clone()).await;

    // 步骤1：从源 Redis 获取 master_replid（通过 INFO REPLICATION）
    tracing::info!("步骤1：从源 Redis 获取 master_replid...");
    let source_master_replid = match fetch_source_master_replid(&config.input).await {
        Ok(replid) => {
            tracing::info!("获取到源 Redis master_replid: {}", replid);
            replid
        }
        Err(e) => {
            tracing::error!("获取源 Redis master_replid 失败: {}，将执行全量同步", e);
            String::new()
        }
    };

    // 步骤2：用 master_replid 查询 checkpoint 获取 offset
    // 先创建 Checkpoint 管理器（连接到目标 Redis）
    let (initial_master_replid, initial_master_offset) = match create_checkpoint_manager(&config.output).await {
        Ok(cm) => {
            tracing::info!("Checkpoint 管理器已创建，连接到目标 Redis");
            let cm = Arc::new(cm);

            // 用已知的 master_replid 查询 checkpoint
            let (replid, offset) = if !source_master_replid.is_empty() {
                tracing::info!("步骤2：使用 master_replid={} 查询 checkpoint...", source_master_replid);
                match cm.get_psync_info(&source_master_replid).await {
                    Ok(Some(o)) => {
                        tracing::info!(
                            "从 checkpoint 获取到 offset={}，将尝试增量同步",
                            o
                        );
                        (source_master_replid, o)
                    }
                    Ok(None) => {
                        tracing::info!("无可用 checkpoint 信息，将执行全量同步（PSYNC ? -1）");
                        (String::new(), -1)
                    }
                    Err(e) => {
                        tracing::warn!("读取 checkpoint 失败: {}，将执行全量同步", e);
                        (String::new(), -1)
                    }
                }
            } else {
                tracing::info!("无 master_replid，将执行全量同步");
                (String::new(), -1)
            };

            syncer.set_checkpoint_manager(cm).await;
            (replid, offset)
        }
        Err(e) => {
            tracing::warn!("创建 Checkpoint 管理器失败（checkpoint 功能将不可用）: {}", e);
            (String::new(), -1)
        }
    };

    // 获取 PSYNC 共享状态（用于传递 PSYNC response 的 offset 和 replid）
    let psync_offset = syncer.get_psync_offset();
    let psync_ready = syncer.get_psync_ready();
    let psync_master_replid = syncer.get_psync_master_replid();

    let input = Arc::new(RedisInput::new(
        Arc::new(config.input.clone()),
        channel.clone(),
        run_id.clone(),
        psync_offset,
        psync_ready,
        initial_master_replid,
        initial_master_offset,
        psync_master_replid,
    ));
    syncer.set_input(input).await;
    
    // 创建并设置 Output
    let output = Arc::new(RedisOutput::new(Arc::new(config.output.clone())));
    syncer.set_output(output).await;
    
    // 创建 AppState
    let app_state = AppState::new(syncer.clone());
    
    // 构建 HTTP Router
    let router = build_router(app_state);
    
    // 获取 HTTP 端口
    let http_port = config.server.http_port;
    let http_addr = format!("0.0.0.0:{}", http_port);
    
    tracing::info!("HTTP 服务地址: {}", http_addr);
    
    // 启动 HTTP 服务
    let listener = tokio::net::TcpListener::bind(&http_addr).await?;
    
    tracing::info!("HTTP 服务启动成功");
    
    // 启动同步器
    tracing::info!("启动同步器...");
    let syncer_clone = syncer.clone();
    let syncer_task = tokio::spawn(async move {
        syncer_clone.run().await
    });
    
    // 等待信号
    tracing::info!("等待终止信号...");
    
    tokio::select! {
        // HTTP 服务
        result = axum::serve(listener, router) => {
            if let Err(e) = result {
                tracing::error!("HTTP 服务错误: {}", e);
            }
        }
        
        // 同步器任务
        result = syncer_task => {
            match result {
                Ok(Ok(())) => tracing::info!("同步器正常完成"),
                Ok(Err(e)) => tracing::error!("同步器错误: {}", e),
                Err(e) => tracing::error!("同步器任务错误: {}", e),
            }
        }
        
        // Ctrl+C 信号
        _ = signal::ctrl_c() => {
            tracing::info!("收到 Ctrl+C 信号");
            syncer.stop().await?;
        }
    }
    
    tracing::info!("Redis Syncer 已停止");
    
    Ok(())
}

/// 创建 Checkpoint 管理器
///
/// 连接到目标 Redis 并创建 CheckpointManager 实例，
/// 用于将同步进度写入 `redis_ha_tool_checkpoint` 和 `redis_ha_tool_checkpoint_hash`。
async fn create_checkpoint_manager(output_config: &redis_syncer::config::OutputConfig) -> Result<CheckpointManager> {
    let addr = &output_config.redis.addresses[0];
    
    // 构建 Redis URL，支持密码认证
    let url = if let Some(password) = &output_config.redis.password {
        if password.is_empty() {
            format!("redis://{}", addr)
        } else {
            // 使用 URL 编码处理密码中的特殊字符
            let encoded: String = url::form_urlencoded::byte_serialize(password.as_bytes()).collect();
            format!("redis://:{}@{}", encoded, addr)
        }
    } else {
        format!("redis://{}", addr)
    };
    
    tracing::debug!("连接目标 Redis（Checkpoint）: {}", addr);
    
    let client = redis::Client::open(url.as_str())
        .map_err(|e| redis_syncer::error::SyncError::Config(
            format!("创建 Redis 客户端失败: {}", e)
        ))?;
    
    let conn = client.get_multiplexed_tokio_connection().await
        .map_err(|e| redis_syncer::error::SyncError::Config(
            format!("连接目标 Redis 失败: {}", e)
        ))?;
    
    tracing::info!("Checkpoint 管理器已连接到目标 Redis: {}", addr);

    Ok(CheckpointManager::new(conn))
}

/// 从源 Redis 获取 master_replid
///
/// 连接到源 Redis，执行 INFO REPLICATION 命令，解析并返回 master_replid。
async fn fetch_source_master_replid(input_config: &redis_syncer::config::InputConfig) -> Result<String> {
    let addr = &input_config.redis.addresses[0];

    // 构建 Redis URL，支持密码认证
    let url = if let Some(password) = &input_config.redis.password {
        if password.is_empty() {
            format!("redis://{}", addr)
        } else {
            let encoded: String = url::form_urlencoded::byte_serialize(password.as_bytes()).collect();
            format!("redis://:{}@{}", encoded, addr)
        }
    } else {
        format!("redis://{}", addr)
    };

    tracing::debug!("连接源 Redis（获取 master_replid）: {}", addr);

    let client = redis::Client::open(url.as_str())
        .map_err(|e| redis_syncer::error::SyncError::Config(
            format!("创建源 Redis 客户端失败: {}", e)
        ))?;

    let mut conn = client.get_multiplexed_tokio_connection().await
        .map_err(|e| redis_syncer::error::SyncError::Config(
            format!("连接源 Redis 失败: {}", e)
        ))?;

    // 执行 INFO REPLICATION
    let info_result: String = redis::cmd("INFO")
        .arg("REPLICATION")
        .query_async(&mut conn)
        .await
        .map_err(|e| redis_syncer::error::SyncError::Protocol(
            format!("执行 INFO REPLICATION 失败: {}", e)
        ))?;

    // 解析 master_replid
    for line in info_result.lines() {
        if line.starts_with("master_replid:") {
            let replid = line.trim_start_matches("master_replid:").trim();
            if !replid.is_empty() && replid != "?" {
                tracing::info!("从源 Redis 获取到 master_replid: {}", replid);
                return Ok(replid.to_string());
            }
        }
    }

    Err(redis_syncer::error::SyncError::Protocol(
        "INFO REPLICATION 响应中未找到 master_replid".to_string()
    ))
}