/// 同一 capability 有多个 provider 时的选择策略（可配置，默认 round-robin）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProviderPolicy {
    #[default]
    RoundRobin,
    First,
}

impl ProviderPolicy {
    pub fn select(&self, providers: &[String], cursor: &mut usize) -> Option<String> {
        if providers.is_empty() {
            return None;
        }
        match self {
            Self::RoundRobin => {
                let index = *cursor % providers.len();
                *cursor = cursor.wrapping_add(1);
                providers.get(index).cloned()
            }
            Self::First => providers.first().cloned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_robin_cycles() {
        let providers = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let mut cursor = 0;
        let picked: Vec<String> = (0..5)
            .map(|_| ProviderPolicy::RoundRobin.select(&providers, &mut cursor).unwrap())
            .collect();
        assert_eq!(picked, vec!["a", "b", "c", "a", "b"]);
    }

    #[test]
    fn first_is_stable_and_empty_is_none() {
        let providers = vec!["a".to_string(), "b".to_string()];
        let mut cursor = 0;
        assert_eq!(ProviderPolicy::First.select(&providers, &mut cursor), Some("a".to_string()));
        assert_eq!(ProviderPolicy::First.select(&providers, &mut cursor), Some("a".to_string()));
        assert_eq!(ProviderPolicy::RoundRobin.select(&[], &mut cursor), None);
    }
}
