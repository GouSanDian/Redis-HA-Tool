//! metric/collector.rs - Prometheus 指标收集器
//!
//! 本文件实现 Prometheus 监控指标的注册和收集。

use prometheus_client::encoding::text::encode;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::registry::Registry;

/// MetricsCollector - 指标收集器
///
/// 管理所有 Prometheus 指标。
pub struct MetricsCollector {
    /// 指标注册表
    registry: Registry,
    
    /// 同步延迟（秒）
    sync_delay_seconds: Gauge<f64>,
    
    /// RDB 回放进度
    rdb_replay_progress: Gauge<f64>,
    
    /// AOF 命令计数
    aof_commands_total: Counter<u64>,
    
    /// 过滤命令计数
    filtered_commands_total: Counter<u64>,
    
    /// 错误计数
    sync_errors_total: Counter<u64>,
}

impl MetricsCollector {
    /// 创建指标收集器
    pub fn new() -> Self {
        let mut registry = Registry::default();
        
        // 创建指标
        let sync_delay_seconds = Gauge::default();
        let rdb_replay_progress = Gauge::default();
        let aof_commands_total = Counter::default();
        let filtered_commands_total = Counter::default();
        let sync_errors_total = Counter::default();
        
        // 注册指标
        registry.register(
            "sync_delay_seconds",
            "同步延迟（秒）",
            sync_delay_seconds.clone(),
        );
        
        registry.register(
            "rdb_replay_progress",
            "RDB 回放进度（百分比）",
            rdb_replay_progress.clone(),
        );
        
        registry.register(
            "aof_commands_total",
            "AOF 命令处理总数",
            aof_commands_total.clone(),
        );
        
        registry.register(
            "filtered_commands_total",
            "过滤命令总数",
            filtered_commands_total.clone(),
        );
        
        registry.register(
            "sync_errors_total",
            "同步错误总数",
            sync_errors_total.clone(),
        );
        
        MetricsCollector {
            registry,
            sync_delay_seconds,
            rdb_replay_progress,
            aof_commands_total,
            filtered_commands_total,
            sync_errors_total,
        }
    }
    
    /// 设置同步延迟
    pub fn set_sync_delay(&self, delay: f64) {
        self.sync_delay_seconds.set(delay);
    }
    
    /// 设置 RDB 回放进度
    pub fn set_rdb_replay_progress(&self, progress: f64) {
        self.rdb_replay_progress.set(progress);
    }
    
    /// 增加 AOF 命令计数
    pub fn inc_aof_commands(&self) {
        self.aof_commands_total.inc();
    }
    
    /// 增加过滤命令计数
    pub fn inc_filtered_commands(&self) {
        self.filtered_commands_total.inc();
    }
    
    /// 增加错误计数
    pub fn inc_sync_errors(&self) {
        self.sync_errors_total.inc();
    }
    
    /// 导出 Prometheus 格式文本
    pub fn export(&self) -> String {
        let mut buffer = String::new();
        encode(&mut buffer, &self.registry).unwrap();
        buffer
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;
    
    /// 测试 MetricsCollector 创建
    #[test]
    fn test_metrics_collector_create() {
        let collector = MetricsCollector::new();
        
        // 验证导出
        let output = collector.export();
        assert!(output.contains("sync_delay_seconds"));
        assert!(output.contains("rdb_replay_progress"));
        assert!(output.contains("aof_commands_total"));
    }
    
    /// 测试指标设置
    #[test]
    fn test_metrics_set() {
        let collector = MetricsCollector::new();
        
        collector.set_sync_delay(1.5);
        collector.set_rdb_replay_progress(50.0);
        
        let output = collector.export();
        assert!(output.contains("1.5"));
        assert!(output.contains("50"));
    }
    
    /// 测试指标增加
    #[test]
    fn test_metrics_inc() {
        let collector = MetricsCollector::new();
        
        collector.inc_aof_commands();
        collector.inc_aof_commands();
        collector.inc_filtered_commands();
        collector.inc_sync_errors();
        
        let output = collector.export();
        assert!(output.contains("2"));  // aof_commands_total
        assert!(output.contains("1"));  // filtered_commands_total
    }
}