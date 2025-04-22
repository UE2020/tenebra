use rand::{distr::Alphanumeric, Rng};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Hash)]
pub enum Permissions {
    ViewOnly,
    FullControl,
}

#[derive(Debug)]
pub struct Keys {
    map: HashMap<String, Permissions>,
}

impl Keys {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn create_key(&mut self, permissions: Permissions) -> String {
        let key: String = rand::rng()
            .sample_iter(&Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();
        self.map.insert(key.clone(), permissions);
        key
    }

    pub fn use_key(&mut self, key: &str) -> Option<Permissions> {
        self.map.remove(&String::from(key))
    }
}
