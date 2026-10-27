use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub microphone: u16,
    pub master: u16,
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub show_avatars: bool,
    pub voice_cleanup: bool,
    pub users: BTreeMap<String, u16>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            microphone: 100,
            master: 100,
            input_device: None,
            output_device: None,
            show_avatars: true,
            voice_cleanup: true,
            users: BTreeMap::new(),
        }
    }
}
impl Preferences {
    pub fn user_gain(&self, server: &str, id: i64) -> u16 {
        self.users
            .get(&format!("{server}|{id}"))
            .copied()
            .unwrap_or(100)
            .min(200)
    }
    pub fn set_user_gain(&mut self, server: &str, id: i64, value: u16) {
        let key = format!("{server}|{id}");
        if value == 100 {
            self.users.remove(&key);
            return;
        }
        self.users.insert(key, value.min(200));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_persistence_and_server_scoping() {
        let mut p: Preferences = serde_json::from_str("{}").unwrap();
        assert!(p.show_avatars);
        assert!(p.voice_cleanup);
        assert_eq!(p.microphone, 100);
        p.set_user_gain("wss://one", 7, 40);
        p.set_user_gain("wss://two", 7, 150);
        p.show_avatars = false;
        p.voice_cleanup = false;
        let p: Preferences = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(p.user_gain("wss://one", 7), 40);
        assert_eq!(p.user_gain("wss://two", 7), 150);
        assert_eq!(p.user_gain("wss://two", 8), 100);
        assert!(!p.show_avatars);
        assert!(!p.voice_cleanup);
    }
}
