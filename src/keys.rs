use rand::{distr::Alphanumeric, Rng};
use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Hash)]
pub enum Permissions {
    ViewOnly,
    FullControl,
}

#[derive(Debug)]
pub struct Keys {
    map: HashMap<String, (Permissions, Instant)>,
}

impl Keys {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn remove_old_keys(&mut self) {
        // Keep keys younger than 1 hr
        self.map.retain(|_, &mut (_, creation_date)| {
            creation_date.elapsed() < Duration::from_secs(3600)
        });
    }

    pub fn create_key(&mut self, permissions: Permissions) -> String {
        let key: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();
        self.map.insert(key.clone(), (permissions, Instant::now()));
        self.remove_old_keys();
        key
    }

    pub fn use_key(&mut self, key: &str) -> Option<Permissions> {
        self.remove_old_keys();
        self.map
            .remove(&String::from(key))
            .map(|(permissions, _)| permissions)
    }
}
