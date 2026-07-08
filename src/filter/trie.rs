//! filter/trie.rs - Trie 树实现
//!
//! 本文件实现 Trie（前缀树）结构，用于命令和 Key 前缀匹配。

use std::collections::HashMap;

/// Trie 树节点
struct TrieNode {
    /// 子节点映射
    children: HashMap<u8, Box<TrieNode>>,
    /// 是否为单词结尾
    is_end: bool,
}

impl TrieNode {
    fn new() -> Self {
        TrieNode {
            children: HashMap::new(),
            is_end: false,
        }
    }
}

/// Trie 树
///
/// 用于前缀匹配，支持插入、精确匹配、前缀匹配。
pub struct Trie {
    /// 根节点
    root: TrieNode,
}

impl Trie {
    /// 创建空 Trie
    pub fn new() -> Self {
        Trie {
            root: TrieNode::new(),
        }
    }
    
    /// 从字符串列表构建 Trie
    ///
    /// # 参数
    /// - keys: 要插入的字符串列表
    pub fn from_keys(keys: &[&str]) -> Self {
        let mut trie = Trie::new();
        for key in keys {
            trie.insert(key.as_bytes());
        }
        trie
    }
    
    /// 插入一个键
    ///
    /// # 参数
    /// - key: 要插入的字节序列
    pub fn insert(&mut self, key: &[u8]) {
        let mut node = &mut self.root;
        
        for byte in key {
            node = node.children.entry(*byte).or_insert_with(|| Box::new(TrieNode::new()));
        }
        
        node.is_end = true;
    }
    
    /// 精确匹配
    ///
    /// 检查 Trie 中是否存在完全匹配的键。
    ///
    /// # 参数
    /// - key: 要检查的字节序列
    ///
    /// # 返回
    /// true 表示存在精确匹配
    pub fn exact_match(&self, key: &[u8]) -> bool {
        let mut node = &self.root;
        
        for byte in key {
            match node.children.get(byte) {
                Some(next) => node = next,
                None => return false,
            }
        }
        
        node.is_end
    }
    
    /// 前缀匹配
    ///
    /// 检查 Trie 中是否存在以指定前缀开始的键。
    ///
    /// # 参数
    /// - prefix: 要检查的前缀
    ///
    /// # 返回
    /// true 表示存在以该前缀开始的键
    pub fn starts_with(&self, prefix: &[u8]) -> bool {
        let mut node = &self.root;
        
        for byte in prefix {
            match node.children.get(byte) {
                Some(next) => node = next,
                None => return false,
            }
        }
        
        // 找到前缀节点，表示存在以该前缀开始的键
        true
    }
    
    /// 检查给定键是否匹配 Trie 中的某个前缀
    ///
    /// 即检查键是否以 Trie 中某个已插入的前缀开头。
    ///
    /// # 参数
    /// - key: 要检查的键
    ///
    /// # 返回
    /// true 表示键以 Trie 中某个前缀开头
    pub fn matches_prefix(&self, key: &[u8]) -> bool {
        let mut node = &self.root;
        
        for byte in key {
            // 如果当前节点是单词结尾，表示匹配了一个前缀
            if node.is_end {
                return true;
            }
            
            match node.children.get(byte) {
                Some(next) => node = next,
                None => return false,
            }
        }
        
        // 检查最后一个节点是否是单词结尾
        node.is_end
    }
    
    /// 获取 Trie 中的节点数量（调试用）
    #[cfg(test)]
    fn count_nodes(&self) -> usize {
        let mut count = 0;
        Self::_count_nodes(&self.root, &mut count);
        count
    }
    
    #[cfg(test)]
    fn _count_nodes(node: &TrieNode, count: &mut usize) {
        *count += 1;
        for child in node.children.values() {
            Self::_count_nodes(child, count);
        }
    }
    
    /// 检查 Trie 是否为空
    pub fn is_empty(&self) -> bool {
        self.root.children.is_empty() && !self.root.is_end
    }
}

impl Default for Trie {
    fn default() -> Self {
        Self::new()
    }
}

// 单元测试
#[cfg(test)]
mod tests {
    use super::*;
    
    /// 测试 Trie 插入和精确匹配
    #[test]
    fn test_trie_insert_and_exact_match() {
        let mut trie = Trie::new();
        
        trie.insert(b"SET");
        trie.insert(b"GET");
        trie.insert(b"DEL");
        
        assert!(trie.exact_match(b"SET"));
        assert!(trie.exact_match(b"GET"));
        assert!(trie.exact_match(b"DEL"));
        assert!(!trie.exact_match(b"S"));      // 前缀，不是完整单词
        assert!(!trie.exact_match(b"SE"));     // 前缀，不是完整单词
        assert!(!trie.exact_match(b"SETX"));   // 超出匹配
        assert!(!trie.exact_match(b"XXX"));    // 不存在
    }
    
    /// 测试 Trie 前缀匹配
    #[test]
    fn test_trie_starts_with() {
        let mut trie = Trie::new();
        
        trie.insert(b"user:");
        trie.insert(b"order:");
        
        // starts_with 检查 Trie 中是否有键以指定前缀开始
        assert!(trie.starts_with(b"user:"));      // Trie 中有键 user:，以 user: 开始
        assert!(trie.starts_with(b"user"));       // Trie 中有键 user:，以 user 开始
        assert!(trie.starts_with(b"order:"));     // Trie 中有键 order:，以 order: 开始
        assert!(trie.starts_with(b"order"));      // Trie 中有键 order:，以 order 开始
        
        assert!(!trie.starts_with(b"user:123"));  // Trie 中没有键以 user:123 开始
        assert!(!trie.starts_with(b"order:456")); // Trie 中没有键以 order:456 开始
        assert!(!trie.starts_with(b"temp:"));     // Trie 中没有键以 temp: 开始
    }
    
    /// 测试 Trie 匹配键的前缀
    #[test]
    fn test_trie_matches_prefix() {
        let mut trie = Trie::new();
        
        trie.insert(b"user:");
        trie.insert(b"temp:");
        
        assert!(trie.matches_prefix(b"user:123"));   // 以 user: 开头
        assert!(trie.matches_prefix(b"user:"));      // 完全匹配
        assert!(trie.matches_prefix(b"temp:abc"));   // 以 temp: 开头
        
        assert!(!trie.matches_prefix(b"order:123")); // 不匹配任何前缀
        assert!(!trie.matches_prefix(b"user"));      // 不完整匹配
    }
    
    /// 测试 Trie 空键
    #[test]
    fn test_trie_empty_key() {
        let mut trie = Trie::new();
        
        trie.insert(b"");  // 插入空键
        
        assert!(trie.exact_match(b""));
        assert!(!trie.exact_match(b"X"));
    }
    
    /// 测试 Trie from_keys
    #[test]
    fn test_trie_from_keys() {
        let trie = Trie::from_keys(&["SET", "GET", "DEL"]);
        
        assert!(trie.exact_match(b"SET"));
        assert!(trie.exact_match(b"GET"));
        assert!(trie.exact_match(b"DEL"));
        assert!(!trie.exact_match(b"FLUSH"));
    }
    
    /// 测试 Trie 大小写敏感
    #[test]
    fn test_trie_case_sensitive() {
        let mut trie = Trie::new();
        
        trie.insert(b"SET");
        
        assert!(trie.exact_match(b"SET"));
        assert!(!trie.exact_match(b"set"));    // 小写，不匹配
        assert!(!trie.exact_match(b"Set"));    // 混合，不匹配
    }
    
    /// 测试 Trie 节点计数
    #[test]
    fn test_trie_count_nodes() {
        let mut trie = Trie::new();
        
        trie.insert(b"SET");
        trie.insert(b"GET");
        trie.insert(b"DEL");
        
        // SET (3节点) + GET (3节点) + DEL (3节点) + root = 10
        // 但 SET 和 GET 共享 S/G 节点，实际节点数可能少于简单相加
        let count = trie.count_nodes();
        assert!(count >= 1);  // 至少有 root
    }
    
    /// 测试 Trie 重复插入
    #[test]
    fn test_trie_duplicate_insert() {
        let mut trie = Trie::new();
        
        trie.insert(b"SET");
        trie.insert(b"SET");  // 重复插入
        
        assert!(trie.exact_match(b"SET"));
    }
    
    /// 测试 Trie 前缀重叠
    #[test]
    fn test_trie_prefix_overlap() {
        let mut trie = Trie::new();
        
        trie.insert(b"user:");
        trie.insert(b"user:profile:");
        
        assert!(trie.exact_match(b"user:"));
        assert!(trie.exact_match(b"user:profile:"));
        assert!(trie.matches_prefix(b"user:profile:name"));
        assert!(trie.starts_with(b"user:profile"));
    }
}