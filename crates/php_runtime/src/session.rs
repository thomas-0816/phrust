//! Request-local CLI session state.

use crate::{PhpArray, Value};

/// Session extension disabled.
pub const PHP_SESSION_DISABLED: i64 = 0;
/// Session extension available but no session is active.
pub const PHP_SESSION_NONE: i64 = 1;
/// Session is active for the current request.
pub const PHP_SESSION_ACTIVE: i64 = 2;

/// Value-free request-local session control plane.
///
/// Exact native session handlers may receive a pointer to this record. PHP
/// arrays and Rust `Value` graphs are deliberately absent, so that capability
/// cannot recover or mutate the baseline session payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeSessionControlState {
    status: i64,
    name: String,
    id: String,
    cache_expire: i64,
    cache_limiter: String,
    module_name: String,
    save_path: String,
    next_id: u64,
    pending_generated_id: Option<String>,
    lazy_load_enabled: bool,
    data_loaded: bool,
    started: bool,
    started_file: Option<String>,
    started_line: u32,
    started_automatically: bool,
    destroyed: bool,
    newly_created: bool,
    destroyed_id: Option<String>,
}

impl Default for NativeSessionControlState {
    fn default() -> Self {
        Self {
            status: PHP_SESSION_NONE,
            name: "PHPSESSID".to_owned(),
            id: String::new(),
            cache_expire: 180,
            cache_limiter: "nocache".to_owned(),
            module_name: "files".to_owned(),
            save_path: String::new(),
            next_id: 1,
            pending_generated_id: None,
            lazy_load_enabled: false,
            data_loaded: true,
            started: false,
            started_file: None,
            started_line: 0,
            started_automatically: false,
            destroyed: false,
            newly_created: false,
            destroyed_id: None,
        }
    }
}

impl NativeSessionControlState {
    #[must_use]
    pub const fn status(&self) -> i64 {
        self.status
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn replace_name(&mut self, name: impl Into<String>) -> String {
        std::mem::replace(&mut self.name, name.into())
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub const fn id_replacement_is_value_free(&self) -> bool {
        !self.lazy_load_enabled
    }

    pub fn replace_id_value_free(&mut self, id: impl Into<String>) -> Option<String> {
        self.id_replacement_is_value_free()
            .then(|| std::mem::replace(&mut self.id, id.into()))
    }

    #[must_use]
    pub const fn cache_expire(&self) -> i64 {
        self.cache_expire
    }

    pub fn replace_cache_expire(&mut self, minutes: i64) -> i64 {
        std::mem::replace(&mut self.cache_expire, minutes)
    }

    #[must_use]
    pub fn cache_limiter(&self) -> &str {
        &self.cache_limiter
    }

    pub fn replace_cache_limiter(&mut self, limiter: impl Into<String>) -> String {
        std::mem::replace(&mut self.cache_limiter, limiter.into())
    }

    #[must_use]
    pub fn module_name(&self) -> &str {
        &self.module_name
    }

    pub fn replace_module_name(&mut self, module_name: impl Into<String>) -> String {
        std::mem::replace(&mut self.module_name, module_name.into())
    }

    #[must_use]
    pub fn save_path(&self) -> &str {
        &self.save_path
    }

    pub fn replace_save_path(&mut self, save_path: impl Into<String>) -> String {
        std::mem::replace(&mut self.save_path, save_path.into())
    }

    #[must_use]
    pub const fn started(&self) -> bool {
        self.started
    }

    #[must_use]
    pub fn started_location(&self) -> Option<(&str, u32)> {
        self.started_file
            .as_deref()
            .map(|file| (file, self.started_line))
    }

    #[must_use]
    pub const fn started_automatically(&self) -> bool {
        self.started_automatically
    }

    #[must_use]
    pub const fn destroyed(&self) -> bool {
        self.destroyed
    }

    #[must_use]
    pub fn destroyed_id(&self) -> Option<&str> {
        self.destroyed_id.as_deref()
    }

    #[must_use]
    pub const fn newly_created(&self) -> bool {
        self.newly_created
    }

    /// Returns true when a transport-backed payload still has to be loaded.
    ///
    /// Exact native session handlers use this to select the single cold
    /// continuation before mutating either the control plane or `$_SESSION`.
    #[must_use]
    pub const fn needs_lazy_load(&self) -> bool {
        self.lazy_load_enabled && !self.data_loaded && !self.id.is_empty()
    }

    pub fn create_id_with_prefix(&mut self, prefix: &str, id_length: usize) -> String {
        let mut id = String::from(prefix);
        id.push_str(&self.next_session_id(id_length));
        id
    }

    /// Activates the value-free half of a session. The caller owns the
    /// authoritative native data and committed snapshots and must update them
    /// before calling this method when `true` is returned.
    pub fn start_value_free(&mut self, id_length: usize, strict_mode: bool) -> bool {
        let generated = self.id.is_empty() || strict_mode;
        if generated {
            self.id = self.next_session_id(id_length);
            self.newly_created = true;
            self.data_loaded = true;
        }
        self.status = PHP_SESSION_ACTIVE;
        self.started = true;
        self.started_automatically = false;
        self.destroyed = false;
        self.destroyed_id = None;
        generated
    }

    /// Replaces the active id without touching the native session payload.
    pub fn regenerate_id_value_free(&mut self, id_length: usize) -> bool {
        if self.status != PHP_SESSION_ACTIVE {
            return false;
        }
        self.id = self.next_session_id(id_length);
        self.destroyed = false;
        self.destroyed_id = None;
        self.started_automatically = false;
        true
    }

    /// Marks the active session destroyed after the caller has cleared both
    /// authoritative native payload owners.
    pub fn destroy_value_free(&mut self) -> bool {
        if self.status != PHP_SESSION_ACTIVE {
            return false;
        }
        self.destroyed_id = Some(self.id.clone());
        self.status = PHP_SESSION_NONE;
        self.id.clear();
        self.data_loaded = true;
        self.destroyed = true;
        self.started_file = None;
        self.started_line = 0;
        self.started_automatically = false;
        true
    }

    /// Closes an active session after the native payload was committed.
    pub fn close_value_free(&mut self) -> bool {
        if self.status != PHP_SESSION_ACTIVE {
            return false;
        }
        self.status = PHP_SESSION_NONE;
        true
    }

    /// Returns whether a payload-only lifecycle operation may proceed.
    #[must_use]
    pub const fn payload_operation_is_active(&self) -> bool {
        self.status == PHP_SESSION_ACTIVE
    }

    fn next_session_id(&mut self, id_length: usize) -> String {
        if let Some(id) = self.pending_generated_id.take() {
            return id;
        }
        let id = format!("phrustcli{:08}", self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        normalized_session_id_length(id, id_length)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct SessionValueState {
    data: PhpArray,
    committed_data: PhpArray,
}

/// Deterministic request-local session storage for CLI execution.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SessionState {
    control: NativeSessionControlState,
    values: SessionValueState,
}

impl SessionState {
    /// A cheap, allocation-free session placeholder for the per-call `VmResult`
    /// success path. Every function return carries a `SessionState`, but inner
    /// calls discard it and the request boundary overwrites the top-level result
    /// from `state.session` (see `Vm::execute`), so this value is never observed.
    /// Unlike [`SessionState::default`] it allocates no default-string heap
    /// buffers (`"PHPSESSID"`/`"nocache"`/`"files"`), removing three allocations
    /// from every call return — the real request session still uses `default()`.
    #[must_use]
    pub fn placeholder() -> Self {
        Self {
            control: NativeSessionControlState {
                status: PHP_SESSION_NONE,
                name: String::new(),
                id: String::new(),
                cache_expire: 180,
                cache_limiter: String::new(),
                module_name: String::new(),
                save_path: String::new(),
                next_id: 1,
                pending_generated_id: None,
                lazy_load_enabled: false,
                data_loaded: true,
                started: false,
                started_file: None,
                started_line: 0,
                started_automatically: false,
                destroyed: false,
                newly_created: false,
                destroyed_id: None,
            },
            values: SessionValueState::default(),
        }
    }

    #[must_use]
    pub const fn native_control(&self) -> &NativeSessionControlState {
        &self.control
    }

    pub fn native_control_mut(&mut self) -> &mut NativeSessionControlState {
        &mut self.control
    }

    /// Returns the current request-local session status.
    #[must_use]
    pub const fn status(&self) -> i64 {
        self.control.status
    }

    /// Returns the current session name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.control.name
    }

    /// Replaces the session name and returns the previous value.
    pub fn replace_name(&mut self, name: impl Into<String>) -> String {
        std::mem::replace(&mut self.control.name, name.into())
    }

    /// Returns the current session id.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.control.id
    }

    /// Replaces the session id and returns the previous value.
    pub fn replace_id(&mut self, id: impl Into<String>) -> String {
        let previous = std::mem::replace(&mut self.control.id, id.into());
        if self.control.lazy_load_enabled && self.control.status != PHP_SESSION_ACTIVE {
            self.values.data = PhpArray::new();
            self.control.data_loaded = self.control.id.is_empty();
            self.control.newly_created = false;
        }
        previous
    }

    /// Returns the current cache expiry in minutes.
    #[must_use]
    pub const fn cache_expire(&self) -> i64 {
        self.control.cache_expire
    }

    /// Replaces the cache expiry and returns the previous value.
    pub fn replace_cache_expire(&mut self, minutes: i64) -> i64 {
        std::mem::replace(&mut self.control.cache_expire, minutes)
    }

    /// Returns the current cache limiter.
    #[must_use]
    pub fn cache_limiter(&self) -> &str {
        &self.control.cache_limiter
    }

    /// Replaces the cache limiter and returns the previous value.
    pub fn replace_cache_limiter(&mut self, limiter: impl Into<String>) -> String {
        std::mem::replace(&mut self.control.cache_limiter, limiter.into())
    }

    /// Returns the current session module name.
    #[must_use]
    pub fn module_name(&self) -> &str {
        &self.control.module_name
    }

    /// Replaces the session module name and returns the previous value.
    pub fn replace_module_name(&mut self, module_name: impl Into<String>) -> String {
        std::mem::replace(&mut self.control.module_name, module_name.into())
    }

    /// Returns the current session save path.
    #[must_use]
    pub fn save_path(&self) -> &str {
        &self.control.save_path
    }

    /// Replaces the session save path and returns the previous value.
    pub fn replace_save_path(&mut self, save_path: impl Into<String>) -> String {
        std::mem::replace(&mut self.control.save_path, save_path.into())
    }

    /// Seeds web-session state loaded by the transport layer.
    #[must_use]
    pub fn seeded(
        name: impl Into<String>,
        id: impl Into<String>,
        data: PhpArray,
        pending_generated_id: Option<String>,
    ) -> Self {
        let mut state = Self::default();
        state.control.name = name.into();
        state.control.id = id.into();
        state.control.pending_generated_id = pending_generated_id;
        state.values.committed_data = data.clone();
        state.values.data = data;
        state
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
        let mut state = Self::default();
        state.control.name = name.into();
        state.control.data_loaded = id.is_empty();
        state.control.lazy_load_enabled = true;
        state.control.id = id;
        state.control.pending_generated_id = pending_generated_id;
        state
    }

    /// Returns true when an existing web session id has not loaded its data yet.
    #[must_use]
    pub const fn needs_lazy_load(&self) -> bool {
        self.control.lazy_load_enabled && !self.control.data_loaded && !self.control.id.is_empty()
    }

    /// Installs session data loaded from the transport session store.
    pub fn load_data(&mut self, data: PhpArray) {
        self.values.committed_data = data.clone();
        self.values.data = data;
        self.control.data_loaded = true;
    }

    /// Returns true when session_start() was called in this request.
    #[must_use]
    pub const fn started(&self) -> bool {
        self.control.started
    }

    /// Returns the display location where session_start() activated the current
    /// session, if it is known.
    #[must_use]
    pub fn started_location(&self) -> Option<(&str, u32)> {
        self.control
            .started_file
            .as_deref()
            .map(|file| (file, self.control.started_line))
    }

    /// Records the display location where session_start() activated the current
    /// session.
    pub fn record_start_location(&mut self, file: impl Into<String>, line: u32) {
        self.control.started_file = Some(file.into());
        self.control.started_line = line;
    }

    /// Returns true when the active session was created by session.auto_start.
    #[must_use]
    pub const fn started_automatically(&self) -> bool {
        self.control.started_automatically
    }

    /// Marks the active session as created by request startup.
    pub fn mark_started_automatically(&mut self) {
        self.control.started_automatically = true;
    }

    /// Returns true when session_destroy() destroyed an active session.
    #[must_use]
    pub const fn destroyed(&self) -> bool {
        self.control.destroyed
    }

    /// Returns the session id destroyed during this request, if any.
    #[must_use]
    pub fn destroyed_id(&self) -> Option<&str> {
        self.control.destroyed_id.as_deref()
    }

    /// Returns true when session_start() created a new session id.
    #[must_use]
    pub const fn newly_created(&self) -> bool {
        self.control.newly_created
    }

    /// Starts a deterministic request-local session.
    ///
    /// Returns `true` when a new deterministic id was generated for this
    /// request, or `false` when an existing id was reused.
    pub fn start(&mut self) -> bool {
        self.start_with_id_length(32)
    }

    /// Starts a deterministic request-local session with a PHP session-id length.
    ///
    /// Returns `true` when a new deterministic id was generated for this
    /// request, or `false` when an existing id was reused.
    pub fn start_with_id_length(&mut self, id_length: usize) -> bool {
        self.start_with_policy(id_length, false)
    }

    /// Starts a deterministic request-local session with PHP strict-ID policy.
    pub fn start_with_policy(&mut self, id_length: usize, strict_mode: bool) -> bool {
        let generated = self.control.id.is_empty() || strict_mode;
        if generated {
            self.control.id = self.next_session_id(id_length);
            self.control.newly_created = true;
            self.control.data_loaded = true;
            self.values.committed_data = PhpArray::new();
        } else {
            self.values.data = self.values.committed_data.clone();
        }
        self.control.status = PHP_SESSION_ACTIVE;
        self.control.started = true;
        self.control.started_automatically = false;
        self.control.destroyed = false;
        self.control.destroyed_id = None;
        generated
    }

    /// Replaces the active session id with a newly generated deterministic id.
    pub fn regenerate_id_with_length(&mut self, id_length: usize) -> bool {
        if self.control.status != PHP_SESSION_ACTIVE {
            return false;
        }
        self.control.id = self.next_session_id(id_length);
        self.control.destroyed = false;
        self.control.destroyed_id = None;
        self.control.started_automatically = false;
        true
    }

    /// Creates a new deterministic session id without activating it.
    pub fn create_id_with_prefix(&mut self, prefix: &str, id_length: usize) -> String {
        let mut id = String::from(prefix);
        id.push_str(&self.next_session_id(id_length));
        id
    }

    fn next_session_id(&mut self, id_length: usize) -> String {
        if let Some(id) = self.control.pending_generated_id.take() {
            return id;
        }
        self.deterministic_session_id(id_length)
    }

    /// Stages a transport-generated id for the next session activation.
    pub fn set_pending_generated_id(&mut self, id: impl Into<String>) {
        self.control.pending_generated_id = Some(id.into());
    }

    fn deterministic_session_id(&mut self, id_length: usize) -> String {
        let id = format!("phrustcli{:08}", self.control.next_id);
        self.control.next_id = self.control.next_id.saturating_add(1);
        normalized_session_id_length(id, id_length)
    }

    /// Destroys the current deterministic CLI session.
    pub fn destroy(&mut self) -> bool {
        if self.control.status != PHP_SESSION_ACTIVE {
            return false;
        }
        self.control.destroyed_id = Some(self.control.id.clone());
        self.control.status = PHP_SESSION_NONE;
        self.control.id.clear();
        self.values.data = PhpArray::new();
        self.values.committed_data = PhpArray::new();
        self.control.data_loaded = true;
        self.control.destroyed = true;
        self.control.started_file = None;
        self.control.started_line = 0;
        self.control.started_automatically = false;
        true
    }

    /// Writes and closes the active deterministic CLI session.
    pub fn write_close(&mut self) -> bool {
        if self.control.status != PHP_SESSION_ACTIVE {
            return false;
        }
        self.values.committed_data = self.values.data.clone();
        self.control.status = PHP_SESSION_NONE;
        true
    }

    /// Discards active in-request changes and closes the session.
    pub fn abort(&mut self) -> bool {
        if self.control.status != PHP_SESSION_ACTIVE {
            return false;
        }
        self.values.data = self.values.committed_data.clone();
        self.control.status = PHP_SESSION_NONE;
        true
    }

    /// Reloads active data from the last committed session snapshot.
    pub fn reset(&mut self) -> bool {
        if self.control.status != PHP_SESSION_ACTIVE {
            return false;
        }
        self.values.data = self.values.committed_data.clone();
        true
    }

    /// Clears the active live session array.
    pub fn unset(&mut self) -> bool {
        if self.control.status != PHP_SESSION_ACTIVE {
            return false;
        }
        self.values.data = PhpArray::new();
        true
    }

    /// Returns a copy of the current `$_SESSION` array.
    #[must_use]
    pub fn data(&self) -> PhpArray {
        self.values.data.clone()
    }

    /// Replaces the stored `$_SESSION` array.
    pub fn set_data(&mut self, data: PhpArray) {
        self.values.data = data;
    }

    /// Returns a copy of the last committed session payload for an explicit
    /// native/cold boundary.
    #[must_use]
    pub fn committed_data(&self) -> PhpArray {
        self.values.committed_data.clone()
    }

    /// Replaces the cold committed snapshot after materializing its
    /// authoritative native owner.
    pub fn set_committed_data(&mut self, data: PhpArray) {
        self.values.committed_data = data;
    }

    /// Returns the stored session data as a PHP value.
    #[must_use]
    pub fn data_value(&self) -> Value {
        Value::Array(self.data())
    }
}

fn normalized_session_id_length(mut id: String, id_length: usize) -> String {
    if id.len() > id_length {
        id.truncate(id_length);
        return id;
    }
    while id.len() < id_length {
        id.push('0');
    }
    id
}

#[must_use]
pub fn native_session_name_is_valid(name: &str) -> bool {
    !name.is_empty()
        && !name.as_bytes().contains(&0)
        && {
            let trimmed = name.trim();
            trimmed.is_empty() || trimmed.parse::<f64>().is_err()
        }
        && !name.bytes().any(|byte| {
            matches!(
                byte,
                b'=' | b',' | b';' | b'.' | b'[' | b' ' | b'\t' | b'\r' | b'\n' | 0x0b | 0x0c
            )
        })
}

#[cfg(test)]
mod tests {
    use super::{PHP_SESSION_ACTIVE, PHP_SESSION_NONE, SessionState};
    use crate::{ArrayKey, PhpArray, PhpString, Value};

    #[test]
    fn session_state_tracks_cli_lifecycle() {
        let mut state = SessionState::default();

        assert_eq!(state.status(), PHP_SESSION_NONE);
        assert_eq!(state.name(), "PHPSESSID");
        assert_eq!(state.id(), "");

        state.start();
        assert_eq!(state.status(), PHP_SESSION_ACTIVE);
        assert_eq!(state.id(), "phrustcli00000001000000000000000");
        assert!(state.started());
        assert!(state.newly_created());

        assert!(state.regenerate_id_with_length(22));
        assert_eq!(state.id(), "phrustcli0000000200000");

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

    #[test]
    fn session_auto_start_marker_resets_on_later_lifecycle_changes() {
        let mut state = SessionState::default();

        state.start();
        state.mark_started_automatically();
        assert!(state.started_automatically());

        assert!(state.write_close());
        state.start();
        assert!(!state.started_automatically());

        state.mark_started_automatically();
        assert!(state.destroy());
        assert!(!state.started_automatically());
    }

    #[test]
    fn session_state_keeps_committed_data_separate_from_live_data() {
        let mut committed = PhpArray::new();
        committed.insert(
            ArrayKey::String(PhpString::from("name")),
            Value::string("committed"),
        );
        let mut state = SessionState::seeded("PHPSESSID", "existing", committed, None);

        state.start();
        let mut live = state.data();
        live.insert(
            ArrayKey::String(PhpString::from("transient")),
            Value::string("reset-me"),
        );
        state.set_data(live);

        assert!(state.reset());
        assert_eq!(
            state.data().get(&ArrayKey::String(PhpString::from("name"))),
            Some(&Value::string("committed"))
        );
        assert_eq!(
            state
                .data()
                .get(&ArrayKey::String(PhpString::from("transient"))),
            None
        );

        let mut live = state.data();
        live.insert(
            ArrayKey::String(PhpString::from("name")),
            Value::string("written"),
        );
        state.set_data(live);
        assert!(state.write_close());

        state.start();
        assert_eq!(
            state.data().get(&ArrayKey::String(PhpString::from("name"))),
            Some(&Value::string("written"))
        );
        assert!(state.unset());
        assert!(state.data().is_empty());
        assert!(state.abort());

        state.start();
        assert_eq!(
            state.data().get(&ArrayKey::String(PhpString::from("name"))),
            Some(&Value::string("written"))
        );
    }
}
