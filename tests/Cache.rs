// src/cache/manager.rs
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Cache {
    data: HashMap<String, String>,
}

impl Cache {
    pub fn new() -> Self {
        Cache {
            data: HashMap::new(),
        }
    }

    pub async fn set(&mut self, key: &str, value: &str) {
        self.data.insert(key.to_string(), value.to_string());
    }

    pub async fn get(&self, key: &str) -> Option<&String> {
        self.data.get(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_set_get() {
        let mut cache = Cache::new();
        
        cache.set("key1", "value1").await;
        assert_eq!(cache.get("key1").await, Some(&"value1".to_string()));
    }
}