//! Request-local CLI session state.

use crate::{PhpArray, Value};

/// Session extension disabled.
pub const PHP_SESSION_DISABLED: i64 = 0;
/// Session extension available but no session is active.
pub const PHP_SESSION_NONE: i64 = 1;
/// Session is active for the current request.
pub const PHP_SESSION_ACTIVE: i64 = 2;

/// Deterministic request-local session storage for CLI execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionState {
    status: i64,
    name: String,
    id: String,
    data: PhpArray,
    cache_expire: i64,
    cache_limiter: String,
    module_name: String,
    save_path: String,
    next_id: u64,
    pending_generated_id: Option<String>,
    lazy_load_enabled: bool,
    data_loaded: bool,
    started: bool,
    destroyed: bool,
    newly_created: bool,
    destroyed_id: Option<String>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            status: PHP_SESSION_NONE,
            name: "PHPSESSID".to_owned(),
            id: String::new(),
            data: PhpArray::new(),
            cache_expire: 180,
            cache_limiter: "nocache".to_owned(),
            module_name: "files".to_owned(),
            save_path: String::new(),
            next_id: 1,
            pending_generated_id: None,
            lazy_load_enabled: false,
            data_loaded: true,
            started: false,
            destroyed: false,
            newly_created: false,
            destroyed_id: None,
        }
    }
}

impl SessionState {
    /// Returns the current request-local session status.
    #[must_use]
    pub const fn status(&self) -> i64 {
        self.status
    }

    /// Returns the current session name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Replaces the session name and returns the previous value.
    pub fn replace_name(&mut self, name: impl Into<String>) -> String {
        std::mem::replace(&mut self.name, name.into())
    }

    /// Returns the current session id.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Replaces the session id and returns the previous value.
    pub fn replace_id(&mut self, id: impl Into<String>) -> String {
        let previous = std::mem::replace(&mut self.id, id.into());
        if self.lazy_load_enabled && self.status != PHP_SESSION_ACTIVE {
            self.data = PhpArray::new();
            self.data_loaded = self.id.is_empty();
            self.newly_created = false;
        }
        previous
    }

    /// Returns the current cache expiry in minutes.
    #[must_use]
    pub const fn cache_expire(&self) -> i64 {
        self.cache_expire
    }

    /// Replaces the cache expiry and returns the previous value.
    pub fn replace_cache_expire(&mut self, minutes: i64) -> i64 {
        std::mem::replace(&mut self.cache_expire, minutes)
    }

    /// Returns the current cache limiter.
    #[must_use]
    pub fn cache_limiter(&self) -> &str {
        &self.cache_limiter
    }

    /// Replaces the cache limiter and returns the previous value.
    pub fn replace_cache_limiter(&mut self, limiter: impl Into<String>) -> String {
        std::mem::replace(&mut self.cache_limiter, limiter.into())
    }

    /// Returns the current session module name.
    #[must_use]
    pub fn module_name(&self) -> &str {
        &self.module_name
    }

    /// Replaces the session module name and returns the previous value.
    pub fn replace_module_name(&mut self, module_name: impl Into<String>) -> String {
        std::mem::replace(&mut self.module_name, module_name.into())
    }

    /// Returns the current session save path.
    #[must_use]
    pub fn save_path(&self) -> &str {
        &self.save_path
    }

    /// Replaces the session save path and returns the previous value.
    pub fn replace_save_path(&mut self, save_path: impl Into<String>) -> String {
        std::mem::replace(&mut self.save_path, save_path.into())
    }

    /// Seeds web-session state loaded by the transport layer.
    #[must_use]
    pub fn seeded(
        name: impl Into<String>,
        id: impl Into<String>,
        data: PhpArray,
        pending_generated_id: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            id: id.into(),
            data,
            pending_generated_id,
            ..Self::default()
        }
    }

    /// Seeds web-session state whose backing store should be loaded only when
    /// PHP activates the session.
    #[must_use]
    pub fn seeded_lazy(
        name: impl Into<String>,
        id: impl Into<String>,
        pending_generated_id: Option<String>,
    ) -> Self {
        let id = id.into();
        Self {
            name: name.into(),
            data_loaded: id.is_empty(),
            lazy_load_enabled: true,
            id,
            pending_generated_id,
            ..Self::default()
        }
    }

    /// Returns true when an existing web session id has not loaded its data yet.
    #[must_use]
    pub const fn needs_lazy_load(&self) -> bool {
        self.lazy_load_enabled && !self.data_loaded && !self.id.is_empty()
    }

    /// Installs session data loaded from the transport session store.
    pub fn load_data(&mut self, data: PhpArray) {
        self.data = data;
        self.data_loaded = true;
    }

    /// Returns true when session_start() was called in this request.
    #[must_use]
    pub const fn started(&self) -> bool {
        self.started
    }

    /// Returns true when session_destroy() destroyed an active session.
    #[must_use]
    pub const fn destroyed(&self) -> bool {
        self.destroyed
    }

    /// Returns the session id destroyed during this request, if any.
    #[must_use]
    pub fn destroyed_id(&self) -> Option<&str> {
        self.destroyed_id.as_deref()
    }

    /// Returns true when session_start() created a new session id.
    #[must_use]
    pub const fn newly_created(&self) -> bool {
        self.newly_created
    }

    /// Starts a deterministic request-local session.
    ///
    /// Returns `true` when a new deterministic id was generated for this
    /// request, or `false` when an existing id was reused.
    pub fn start(&mut self) -> bool {
        let generated = self.id.is_empty();
        if self.id.is_empty() {
            self.id = self.pending_generated_id.take().unwrap_or_else(|| {
                let id = format!("phrustcli{:08}", self.next_id);
                self.next_id = self.next_id.saturating_add(1);
                id
            });
            self.newly_created = true;
            self.data_loaded = true;
        }
        self.status = PHP_SESSION_ACTIVE;
        self.started = true;
        self.destroyed = false;
        self.destroyed_id = None;
        generated
    }

    /// Destroys the current deterministic CLI session.
    pub fn destroy(&mut self) -> bool {
        if self.status != PHP_SESSION_ACTIVE {
            return false;
        }
        self.destroyed_id = Some(self.id.clone());
        self.status = PHP_SESSION_NONE;
        self.id.clear();
        self.data = PhpArray::new();
        self.data_loaded = true;
        self.destroyed = true;
        true
    }

    /// Writes and closes the active deterministic CLI session.
    pub fn write_close(&mut self) -> bool {
        if self.status == PHP_SESSION_ACTIVE {
            self.status = PHP_SESSION_NONE;
        }
        true
    }

    /// Returns a copy of the current `$_SESSION` array.
    #[must_use]
    pub fn data(&self) -> PhpArray {
        self.data.clone()
    }

    /// Replaces the stored `$_SESSION` array.
    pub fn set_data(&mut self, data: PhpArray) {
        self.data = data;
    }

    /// Returns the stored session data as a PHP value.
    #[must_use]
    pub fn data_value(&self) -> Value {
        Value::Array(self.data())
    }
}

#[cfg(test)]
mod tests {
    use super::{PHP_SESSION_ACTIVE, PHP_SESSION_NONE, SessionState};

    #[test]
    fn session_state_tracks_cli_lifecycle() {
        let mut state = SessionState::default();

        assert_eq!(state.status(), PHP_SESSION_NONE);
        assert_eq!(state.name(), "PHPSESSID");
        assert_eq!(state.id(), "");

        state.start();
        assert_eq!(state.status(), PHP_SESSION_ACTIVE);
        assert_eq!(state.id(), "phrustcli00000001");
        assert!(state.started());
        assert!(state.newly_created());

        assert!(state.destroy());
        assert_eq!(state.status(), PHP_SESSION_NONE);
        assert_eq!(state.id(), "");
        assert!(state.destroyed());
        assert!(!state.destroy());
    }

    #[test]
    fn session_state_can_be_seeded_for_web_requests() {
        let mut state = SessionState::seeded(
            "APPSESSID",
            "",
            crate::PhpArray::new(),
            Some("generated".to_string()),
        );

        assert_eq!(state.name(), "APPSESSID");
        state.start();
        assert_eq!(state.id(), "generated");
        assert!(state.newly_created());
    }
}
