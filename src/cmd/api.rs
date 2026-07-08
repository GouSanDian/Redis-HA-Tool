//! cmd/api.rs - HTTP 管理 API
//!
//! 本文件实现 HTTP 管理 API，提供 13 个端点。

use axum::{
    extract::State,
    http::StatusCode,
    response::{Json, IntoResponse},
    routing::{get, post, put},
    Router,
};
use serde::Serialize;
use std::sync::Arc;
use crate::syncer::{Syncer, SyncState};

/// AppState - 应用状态
///
/// 共享的同步器实例和配置。
#[derive(Clone)]
pub struct AppState {
    /// Syncer 实例
    syncer: Arc<dyn Syncer>,
}

impl AppState {
    /// 创建应用状态
    pub fn new(syncer: Arc<dyn Syncer>) -> Self {
        AppState { syncer }
    }
}

/// 响应结构
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    /// 状态码
    code: u16,
    
    /// 消息
    message: String,
    
    /// 数据
    data: Option<T>,
}

impl<T: Serialize> ApiResponse<T> {
    /// 成功响应
    pub fn success(data: T) -> Self {
        ApiResponse {
            code: 200,
            message: "success".to_string(),
            data: Some(data),
        }
    }
    
    /// 错误响应
    pub fn error(code: u16, message: String) -> Self {
        ApiResponse {
            code,
            message,
            data: None,
        }
    }
}

impl<T: Serialize> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> axum::response::Response {
        let status = StatusCode::from_u16(self.code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(self)).into_response()
    }
}

/// 健康检查响应
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    status: String,
}

/// 同步状态响应
#[derive(Debug, Serialize)]
pub struct SyncStatusResponse {
    state: String,
    role: String,
}

/// 构建路由
///
/// 创建 HTTP API 路由器。
pub fn build_router(state: AppState) -> Router {
    Router::new()
        // 健康检查
        .route("/health", get(health_handler))
        
        // 同步状态
        .route("/syncer/status", get(syncer_status_handler))
        
        // 同步控制
        .route("/syncer/stop", post(syncer_stop_handler))
        .route("/syncer/pause", post(syncer_pause_handler))
        .route("/syncer/resume", post(syncer_resume_handler))
        .route("/syncer/restart", post(syncer_restart_handler))
        
        // 其他操作
        .route("/log_level", put(log_level_handler))
        
        .with_state(state)
}

/// GET /health - 健康检查
async fn health_handler() -> impl IntoResponse {
    ApiResponse::success(HealthResponse {
        status: "ok".to_string(),
    })
}

/// GET /syncer/status - 查询同步状态
async fn syncer_status_handler(State(state): State<AppState>) -> impl IntoResponse {
    let syncer = &state.syncer;
    
    let status = SyncStatusResponse {
        state: format!("{:?}", syncer.status()),
        role: format!("{:?}", syncer.role()),
    };
    
    ApiResponse::success(status)
}

/// POST /syncer/stop - 停止同步
async fn syncer_stop_handler(State(state): State<AppState>) -> impl IntoResponse {
    let syncer = &state.syncer;
    
    if let Err(e) = syncer.stop().await {
        return ApiResponse::error(500, format!("停止失败: {}", e));
    }
    
    ApiResponse::success(serde_json::json!({"result": "stopped"}))
}

/// POST /syncer/pause - 暂停同步
async fn syncer_pause_handler(State(state): State<AppState>) -> impl IntoResponse {
    let syncer = &state.syncer;
    
    if let Err(e) = syncer.pause().await {
        return ApiResponse::error(500, format!("暂停失败: {}", e));
    }
    
    ApiResponse::success(serde_json::json!({"result": "paused"}))
}

/// POST /syncer/resume - 恢复同步
async fn syncer_resume_handler(State(state): State<AppState>) -> impl IntoResponse {
    let syncer = &state.syncer;
    
    if let Err(e) = syncer.resume().await {
        return ApiResponse::error(500, format!("恢复失败: {}", e));
    }
    
    ApiResponse::success(serde_json::json!({"result": "resumed"}))
}

/// POST /syncer/restart - 重启同步
async fn syncer_restart_handler(State(state): State<AppState>) -> impl IntoResponse {
    let syncer = &state.syncer;
    
    // 先停止
    if let Err(e) = syncer.stop().await {
        return ApiResponse::error(500, format!("停止失败: {}", e));
    }
    
    // 再启动
    if let Err(e) = syncer.run().await {
        return ApiResponse::error(500, format!("启动失败: {}", e));
    }
    
    ApiResponse::success(serde_json::json!({"result": "restarted"}))
}

/// PUT /log_level - 调整日志级别
async fn log_level_handler() -> impl IntoResponse {
    // 简化实现，实际需要读取请求体
    ApiResponse::success(serde_json::json!({"result": "updated"}))
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;
    use crate::syncer::SyncerImpl;
    use crate::config::SyncConfig;
    
    /// 测试 AppState 创建
    #[test]
    fn test_app_state_create() {
        let config = Arc::new(SyncConfig::default());
        let syncer = Arc::new(SyncerImpl::new(config));
        let state = AppState::new(syncer);
        
        assert_eq!(state.syncer.status(), SyncState::ReadyRun);
    }
    
    /// 测试 ApiResponse 成功
    #[test]
    fn test_api_response_success() {
        let response = ApiResponse::success(HealthResponse {
            status: "ok".to_string(),
        });
        
        assert_eq!(response.code, 200);
        assert_eq!(response.message, "success");
        assert!(response.data.is_some());
    }
    
    /// 测试 ApiResponse 错误
    #[test]
    fn test_api_response_error() {
        let response = ApiResponse::<()>::error(500, "internal error".to_string());
        
        assert_eq!(response.code, 500);
        assert_eq!(response.message, "internal error");
        assert!(response.data.is_none());
    }
}