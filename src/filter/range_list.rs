//! filter/range_list.rs - Slot 范围列表实现
//!
//! 本文件实现 RangeList，用于高效检查 Slot 是否在指定范围内。

use crate::config::SlotRange;

/// Slot 范围列表
///
/// 维护一组有序、不重叠的范围，支持快速包含检查。
pub struct RangeList {
    /// 范围列表，按起始值有序存储
    ranges: Vec<(u16, u16)>,  // (start, end)
}

impl RangeList {
    /// 创建空范围列表
    pub fn new() -> Self {
        RangeList {
            ranges: Vec::new(),
        }
    }
    
    /// 从 SlotRange 配置构建
    ///
    /// # 参数
    /// - slot_ranges: Slot 范围配置列表
    pub fn from_config(slot_ranges: &[SlotRange]) -> Self {
        let mut ranges: Vec<(u16, u16)> = slot_ranges
            .iter()
            .map(|r| (r.start, r.end))
            .collect();
        
        // 排序并合并重叠范围
        if !ranges.is_empty() {
            ranges.sort_by_key(|r| r.0);
            
            let mut merged: Vec<(u16, u16)> = Vec::new();
            let mut current = ranges[0];
            
            for i in 1..ranges.len() {
                let next = ranges[i];
                
                // 如果重叠或相邻，合并
                if next.0 <= current.1 + 1 {
                    current.1 = std::cmp::max(current.1, next.1);
                } else {
                    merged.push(current);
                    current = next;
                }
            }
            
            merged.push(current);
            ranges = merged;
        }
        
        RangeList { ranges }
    }
    
    /// 从原始范围列表构建
    ///
    /// # 参数
    /// - ranges: (start, end) 范围列表
    pub fn from_ranges(ranges: Vec<(u16, u16)>) -> Self {
        RangeList::from_config(
            &ranges.iter()
                .map(|(s, e)| SlotRange::new(*s, *e))
                .collect::<Vec<_>>()
        )
    }
    
    /// 检查 Slot 是否在范围内
    ///
    /// # 参数
    /// - slot: 要检查的 Slot 值
    ///
    /// # 返回
    /// true 表示 slot 在某个范围内
    pub fn contains(&self, slot: u16) -> bool {
        // 使用二分查找（因为范围有序）
        for (start, end) in &self.ranges {
            if slot >= *start && slot <= *end {
                return true;
            }
            
            // 如果 slot < start，后面的范围都更大，可以提前退出
            if slot < *start {
                break;
            }
        }
        
        false
    }
    
    /// 添加范围
    ///
    /// # 参数
    /// - start: 范围起始
    /// - end: 范围结束
    pub fn add(&mut self, start: u16, end: u16) {
        let new_range = SlotRange::new(start, end);
        let mut new_ranges: Vec<SlotRange> = self.ranges
            .iter()
            .map(|(s, e)| SlotRange::new(*s, *e))
            .collect();
        new_ranges.push(new_range);
        
        // 重建（合并）
        *self = RangeList::from_config(&new_ranges);
    }
    
    /// 获取范围数量
    pub fn len(&self) -> usize {
        self.ranges.len()
    }
    
    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }
    
    /// 获取所有范围
    pub fn ranges(&self) -> &[(u16, u16)] {
        &self.ranges
    }
    
    /// 计算总覆盖大小
    pub fn total_coverage(&self) -> u32 {
        self.ranges.iter()
            .map(|(start, end)| (end - start + 1) as u32)
            .sum()
    }
}

impl Default for RangeList {
    fn default() -> Self {
        Self::new()
    }
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;
    
    /// 测试 RangeList 单范围包含
    #[test]
    fn test_range_list_single_range() {
        let range_list = RangeList::from_ranges(vec![(0, 100)]);
        
        assert!(range_list.contains(0));      // 起始边界
        assert!(range_list.contains(50));     // 中间
        assert!(range_list.contains(100));    // 结束边界
        
        assert!(!range_list.contains(101));   // 超出
        assert!(!range_list.contains(65535)); // 远超出
    }
    
    /// 测试 RangeList 多范围包含
    #[test]
    fn test_range_list_multiple_ranges() {
        let range_list = RangeList::from_ranges(vec![(0, 100), (200, 300)]);
        
        assert!(range_list.contains(50));     // 第一个范围
        assert!(range_list.contains(250));    // 第二个范围
        
        assert!(!range_list.contains(150));   // 间隙
        assert!(!range_list.contains(301));   // 超出
    }
    
    /// 测试 RangeList 空范围
    #[test]
    fn test_range_list_empty() {
        let range_list = RangeList::new();
        
        assert!(!range_list.contains(0));
        assert!(!range_list.contains(100));
        assert!(range_list.is_empty());
    }
    
    /// 测试 RangeList 从配置构建
    #[test]
    fn test_range_list_from_config() {
        let config_ranges = vec![
            SlotRange::new(0, 100),
            SlotRange::new(200, 300),
        ];
        
        let range_list = RangeList::from_config(&config_ranges);
        
        assert_eq!(range_list.len(), 2);
        assert!(range_list.contains(50));
        assert!(range_list.contains(250));
    }
    
    /// 测试 RangeList 合并重叠范围
    #[test]
    fn test_range_list_merge_overlap() {
        let range_list = RangeList::from_ranges(vec![
            (0, 100),
            (50, 150),    // 与第一个重叠
            (200, 300),
        ]);
        
        // 应合并为 (0, 150) 和 (200, 300)
        assert_eq!(range_list.len(), 2);
        assert!(range_list.contains(120));    // 在合并后的范围内
        assert!(range_list.contains(250));
    }
    
    /// 测试 RangeList 合并相邻范围
    #[test]
    fn test_range_list_merge_adjacent() {
        let range_list = RangeList::from_ranges(vec![
            (0, 100),
            (101, 200),   // 相邻
        ]);
        
        // 应合并为 (0, 200)
        assert_eq!(range_list.len(), 1);
        assert!(range_list.contains(100));
        assert!(range_list.contains(101));
        assert!(range_list.contains(150));
    }
    
    /// 测试 RangeList 添加范围
    #[test]
    fn test_range_list_add() {
        let mut range_list = RangeList::new();
        
        range_list.add(0, 100);
        
        assert_eq!(range_list.len(), 1);
        assert!(range_list.contains(50));
        
        range_list.add(200, 300);
        
        assert_eq!(range_list.len(), 2);
        assert!(range_list.contains(250));
    }
    
    /// 测试 RangeList 总覆盖大小
    #[test]
    fn test_range_list_total_coverage() {
        let range_list = RangeList::from_ranges(vec![(0, 100), (200, 300)]);
        
        // (100-0+1) + (300-200+1) = 101 + 101 = 202
        assert_eq!(range_list.total_coverage(), 202);
    }
    
    /// 测试 RangeList 边界值
    #[test]
    fn test_range_list_boundary() {
        let range_list = RangeList::from_ranges(vec![(0, 0), (65535, 65535)]);
        
        assert!(range_list.contains(0));
        assert!(range_list.contains(65535));
        assert!(!range_list.contains(1));
        assert!(!range_list.contains(65534));
    }
    
    /// 测试 RangeList 大范围
    #[test]
    fn test_range_list_full_range() {
        let range_list = RangeList::from_ranges(vec![(0, 16383)]);
        
        // Redis Cluster 有 16384 个 slot (0-16383)
        assert_eq!(range_list.total_coverage(), 16384);
        assert!(range_list.contains(8191));
    }
}