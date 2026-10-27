use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Nonce,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const MAX_TEXT: usize = 4096;
pub const MAX_FRAME: usize = 32768;
/// Includes JSON escaping for a full page of fifty maximum-size messages.
pub const MAX_EVENT: usize = 512 * 1024;
pub const MAX_CALL: usize = 8;
pub const MAX_OPUS: usize = 1275;
pub const PAGE: usize = 50;
pub const FRAME: usize = 960;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Register {
        username: String,
        password: String,
    },
    Login {
        username: String,
        password: String,
    },
    Resume {
        token: String,
    },
    Logout {},
    Sync {},
    FriendRequest {
        username: String,
    },
    FriendAccept {
        user: i64,
    },
    Conversation {
        users: Vec<i64>,
    },
    History {
        conversation: i64,
        before: Option<i64>,
    },
    Send {
        conversation: i64,
        nonce: String,
        text: String,
    },
    Call {
        conversation: i64,
    },
    Invite {
        call: u64,
        user: i64,
    },
    Join {
        call: u64,
    },
    Decline {
        call: u64,
    },
    Leave {},
    Controls {
        mic_muted: bool,
        deafened: bool,
    },
    Toggle {},
    VoiceTransport {
        udp: bool,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub username: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: i64,
    pub users: Vec<User>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: i64,
    pub conversation: i64,
    pub sender: User,
    pub timestamp: u64,
    pub nonce: String,
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Participant {
    pub user: User,
    pub mic_muted: bool,
    pub deafened: bool,
    pub labeled: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Call {
    pub id: u64,
    pub conversation: i64,
    pub participants: Vec<Participant>,
    pub invited: Vec<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub friends: Vec<User>,
    pub incoming: Vec<User>,
    pub conversations: Vec<Conversation>,
    pub calls: Vec<Call>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Auth {
        user: User,
        token: String,
        voice_id: [u8; 16],
        voice_key: [u8; 32],
        udp_port: u16,
    },
    State {
        state: Snapshot,
    },
    History {
        conversation: i64,
        messages: Vec<ChatMessage>,
    },
    Message {
        message: ChatMessage,
    },
    Ack {
        nonce: String,
        id: i64,
    },
    Rejected {
        nonce: String,
        message: String,
    },
    Error {
        message: String,
    },
    LoggedOut,
    Pong,
}
pub fn valid_username(s: &str) -> bool {
    (3..=24).contains(&s.len()) && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
}
/// Per-direction keys, monotonic 64-bit packet counters and authenticated session header.
pub struct VoiceCipher {
    cipher: ChaCha20Poly1305,
    id: [u8; 16],
    next: u64,
    highest: Option<u64>,
    seen: u64,
}
impl VoiceCipher {
    pub fn new(key: &[u8; 32], id: [u8; 16], direction: &[u8]) -> Self {
        let mut h = Sha256::new();
        h.update(key);
        h.update(direction);
        Self {
            cipher: ChaCha20Poly1305::new(&h.finalize()),
            id,
            next: 0,
            highest: None,
            seen: 0,
        }
    }
    pub fn seal(&mut self, body: &[u8]) -> Option<Vec<u8>> {
        if body.len() > MAX_OPUS + 24 || self.next == u64::MAX {
            return None;
        }
        let seq = self.next;
        self.next += 1;
        let mut header = self.id.to_vec();
        header.extend(seq.to_be_bytes());
        let mut n = [0u8; 12];
        n[4..].copy_from_slice(&seq.to_be_bytes());
        let encrypted = self
            .cipher
            .encrypt(
                Nonce::from_slice(&n),
                Payload {
                    msg: body,
                    aad: &header,
                },
            )
            .ok()?;
        header.extend(encrypted);
        Some(header)
    }
    pub fn open(&mut self, data: &[u8]) -> Option<Vec<u8>> {
        if data.len() < 40 || data.len() > MAX_OPUS + 64 || data[..16] != self.id {
            return None;
        }
        let seq = u64::from_be_bytes(data[16..24].try_into().ok()?);
        if let Some(high) = self.highest {
            if seq <= high && (high - seq >= 64 || self.seen & (1 << (high - seq)) != 0) {
                return None;
            }
        }
        let mut n = [0u8; 12];
        n[4..].copy_from_slice(&seq.to_be_bytes());
        let body = self
            .cipher
            .decrypt(
                Nonce::from_slice(&n),
                Payload {
                    msg: &data[24..],
                    aad: &data[..24],
                },
            )
            .ok()?;
        match self.highest {
            None => {
                self.highest = Some(seq);
                self.seen = 1;
            }
            Some(h) if seq > h => {
                let d = seq - h;
                self.seen = if d >= 64 { 1 } else { (self.seen << d) | 1 };
                self.highest = Some(seq);
            }
            Some(h) => self.seen |= 1 << (h - seq),
        }
        Some(body)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authenticated_replay_and_reordering() {
        let mut a = VoiceCipher::new(&[7; 32], [1; 16], b"up");
        let mut b = VoiceCipher::new(&[7; 32], [1; 16], b"up");
        let x = a.seal(b"one").unwrap();
        let y = a.seal(b"two").unwrap();
        assert_eq!(b.open(&y).unwrap(), b"two");
        assert_eq!(b.open(&x).unwrap(), b"one");
        assert!(b.open(&x).is_none());
        let mut z = a.seal(b"three").unwrap();
        z[25] ^= 1;
        assert!(b.open(&z).is_none());
        assert!(b.open(&[0; 12]).is_none());
    }
    #[test]
    fn direction_and_names() {
        let x = VoiceCipher::new(&[0; 32], [0; 16], b"up")
            .seal(b"a")
            .unwrap();
        assert!(VoiceCipher::new(&[0; 32], [0; 16], b"down")
            .open(&x)
            .is_none());
        assert!(valid_username("Alex_1"));
        assert!(!valid_username("a"));
        assert!(!valid_username("ab cd"));
    }
    #[test]
    fn malformed_and_old_packets_cannot_advance_replay_window() {
        let mut send = VoiceCipher::new(&[8; 32], [9; 16], b"up");
        let mut receive = VoiceCipher::new(&[8; 32], [9; 16], b"up");
        let old = send.seal(b"old").unwrap();
        for _ in 0..70 {
            let packet = send.seal(b"current").unwrap();
            assert_eq!(receive.open(&packet).unwrap(), b"current");
        }
        assert!(receive.open(&old).is_none());
        for length in 0..1400 {
            assert!(receive.open(&vec![0; length]).is_none());
        }
        let valid = send.seal(b"still valid").unwrap();
        assert_eq!(receive.open(&valid).unwrap(), b"still valid");
        assert!(serde_json::from_str::<Request>(r#"{"op":"join","call":-1}"#).is_err());
        assert!(serde_json::from_str::<Request>(r#"{"op":"toggle","admin":true}"#).is_err());
    }
}
