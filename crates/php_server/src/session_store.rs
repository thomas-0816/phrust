use php_runtime::api::{PhpArray, PhpString, UnserializeOptions, Value, serialize, unserialize};
use std::{
    collections::HashMap,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Debug)]
pub struct RuntimeSessionStore {
    entries: Mutex<HashMap<String, SessionEntry>>,
    max_entries: usize,
    ttl: Option<Duration>,
}

pub type SessionStore = RuntimeSessionStore;

#[derive(Clone, Debug)]
struct SessionEntry {
    payload: Vec<u8>,
    last_access: Instant,
}

#[derive(Debug)]
pub enum SessionStoreError {
    Io(io::Error),
    InvalidId,
    Decode(String),
    Encode(String),
    Unavailable,
}

impl std::fmt::Display for SessionStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::InvalidId => f.write_str("invalid session id"),
            Self::Decode(message) => write!(f, "session decode failed: {message}"),
            Self::Encode(message) => write!(f, "session encode failed: {message}"),
            Self::Unavailable => f.write_str("session store unavailable"),
        }
    }
}

impl std::error::Error for SessionStoreError {}

impl From<io::Error> for SessionStoreError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl RuntimeSessionStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let _ = root.into();
        Self {
            entries: Mutex::new(HashMap::new()),
            max_entries: 4096,
            ttl: None,
        }
    }

    #[must_use]
    pub fn with_limits(
        root: impl Into<PathBuf>,
        max_entries: usize,
        ttl: Option<Duration>,
    ) -> Self {
        let _ = root.into();
        Self {
            entries: Mutex::new(HashMap::new()),
            max_entries: max_entries.max(1),
            ttl,
        }
    }

    pub fn ensure_ready(&self) -> Result<(), SessionStoreError> {
        Ok(())
    }

    pub fn load(&self, id: &str) -> Result<PhpArray, SessionStoreError> {
        validate_id(id)?;
        let now = Instant::now();
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| SessionStoreError::Unavailable)?;
        self.prune_expired_locked(&mut entries, now);
        let Some(entry) = entries.get_mut(id) else {
            return Ok(PhpArray::new());
        };
        entry.last_access = now;
        decode_session_payload(entry.payload.clone())
    }

    pub fn save(&self, id: &str, data: &PhpArray) -> Result<(), SessionStoreError> {
        validate_id(id)?;
        let now = Instant::now();
        let payload = encode_session_payload(data)?;
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| SessionStoreError::Unavailable)?;
        self.prune_expired_locked(&mut entries, now);
        if !entries.contains_key(id) && entries.len() >= self.max_entries {
            evict_oldest(&mut entries);
        }
        entries.insert(
            id.to_string(),
            SessionEntry {
                payload,
                last_access: now,
            },
        );
        Ok(())
    }

    pub fn delete(&self, id: &str) -> Result<(), SessionStoreError> {
        validate_id(id)?;
        self.entries
            .lock()
            .map_err(|_| SessionStoreError::Unavailable)?
            .remove(id);
        Ok(())
    }

    fn prune_expired_locked(&self, entries: &mut HashMap<String, SessionEntry>, now: Instant) {
        let Some(ttl) = self.ttl else {
            return;
        };
        entries.retain(|_, entry| now.duration_since(entry.last_access) <= ttl);
    }
}

fn validate_id(id: &str) -> Result<(), SessionStoreError> {
    if valid_session_id(id) {
        Ok(())
    } else {
        Err(SessionStoreError::InvalidId)
    }
}

fn evict_oldest(entries: &mut HashMap<String, SessionEntry>) {
    if let Some(oldest) = entries
        .iter()
        .min_by_key(|(_, entry)| entry.last_access)
        .map(|(id, _)| id.clone())
    {
        entries.remove(&oldest);
    }
}

fn decode_session_payload(bytes: Vec<u8>) -> Result<PhpArray, SessionStoreError> {
    let value = unserialize(
        &PhpString::from_bytes(bytes),
        UnserializeOptions {
            max_bytes: 1_048_576,
            ..UnserializeOptions::default()
        },
    )
    .map_err(|error| SessionStoreError::Decode(error.message().to_string()))?;
    match value {
        Value::Array(array) => Ok(array),
        _ => Err(SessionStoreError::Decode(
            "session payload is not an array".to_string(),
        )),
    }
}

fn encode_session_payload(data: &PhpArray) -> Result<Vec<u8>, SessionStoreError> {
    serialize(&Value::Array(data.clone()))
        .map_err(|error| SessionStoreError::Encode(error.message().to_string()))
        .map(PhpString::into_bytes)
}

pub fn valid_session_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b','))
}

pub fn generate_session_id() -> io::Result<String> {
    let mut bytes = [0u8; 24];
    fs::File::open(Path::new("/dev/urandom")).and_then(|mut file| file.read_exact(&mut bytes))?;
    Ok(hex_bytes(&bytes))
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = Vec::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize]);
        output.push(HEX[(byte & 0x0f) as usize]);
    }
    String::from_utf8(output).expect("hex is utf-8")
}

#[cfg(test)]
mod tests {
    use super::{SessionStore, generate_session_id, valid_session_id};
    use php_runtime::api::{ArrayKey, PhpArray, PhpString, Value};

    #[test]
    fn session_ids_are_strict_path_segments() {
        assert!(valid_session_id("abcDEF0123-,"));
        assert!(!valid_session_id(""));
        assert!(!valid_session_id("../bad"));
        assert!(!valid_session_id("bad/slash"));
        assert!(!valid_session_id("bad\nid"));
    }

    #[test]
    fn session_store_roundtrips_php_array_payloads() {
        let root =
            std::env::temp_dir().join(format!("phrust-session-store-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let store = SessionStore::new(&root);
        let mut data = PhpArray::new();
        data.insert(
            ArrayKey::String(PhpString::from_test_str("n")),
            Value::Int(2),
        );

        store.save("abc123", &data).expect("save session");
        assert_eq!(store.load("abc123").expect("load session"), data);
        store.delete("abc123").expect("delete session");
        assert!(store.load("abc123").expect("load missing").is_empty());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn session_store_is_process_local_and_does_not_create_files() {
        let root = std::env::temp_dir().join(format!(
            "phrust-session-store-memory-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let store = SessionStore::new(&root);
        let mut data = PhpArray::new();
        data.insert(
            ArrayKey::String(PhpString::from_test_str("flag")),
            Value::Bool(true),
        );

        store.ensure_ready().expect("session store ready");
        store.save("abc123", &data).expect("save session");

        assert!(!root.exists(), "memory session store must not create files");
        assert_eq!(store.load("abc123").expect("load session"), data);
    }

    #[test]
    fn session_store_evicts_old_entries_when_bounded() {
        let store = SessionStore::with_limits("unused", 1, None);
        let mut first = PhpArray::new();
        first.insert(
            ArrayKey::String(PhpString::from_test_str("n")),
            Value::Int(1),
        );
        let mut second = PhpArray::new();
        second.insert(
            ArrayKey::String(PhpString::from_test_str("n")),
            Value::Int(2),
        );

        store.save("first", &first).expect("save first");
        store.save("second", &second).expect("save second");

        assert!(store.load("first").expect("load first").is_empty());
        assert_eq!(store.load("second").expect("load second"), second);
    }

    #[test]
    fn generated_session_ids_are_valid() {
        let id = generate_session_id().expect("generate session id");
        assert!(valid_session_id(&id), "{id}");
    }
}
