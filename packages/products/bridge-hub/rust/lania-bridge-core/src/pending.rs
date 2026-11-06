use std::collections::HashMap;

/// `request id → 发起方 endpointId` 关联表。
///
/// correlation id 由端点骨架生成（opaque 字符串），Hub 只负责登记与回投。
#[derive(Debug, Default)]
pub struct PendingTable {
    entries: HashMap<String, String>,
}

impl PendingTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, id: String, origin: String) {
        self.entries.insert(id, origin);
    }

    /// 取出并移除发起方；未知 id 返回 None。
    pub fn take(&mut self, id: &str) -> Option<String> {
        self.entries.remove(id)
    }

    /// 端点下线时清理它发起的、尚未回投的请求。
    pub fn remove_origin(&mut self, origin: &str) -> Vec<String> {
        let stale: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, value)| value.as_str() == origin)
            .map(|(key, _)| key.clone())
            .collect();
        for id in &stale {
            self.entries.remove(id);
        }
        stale
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correlates_and_cleans_up() {
        let mut table = PendingTable::new();
        table.insert("cli-1".into(), "cli".into());
        table.insert("py-1".into(), "py".into());
        assert_eq!(table.take("cli-1"), Some("cli".to_string()));
        assert_eq!(table.take("cli-1"), None);
        assert_eq!(table.len(), 1);
        assert_eq!(table.remove_origin("py"), vec!["py-1".to_string()]);
        assert!(table.is_empty());
    }
}
