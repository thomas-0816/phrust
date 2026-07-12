//! Runtime services passed to internal builtins.

use super::request_state::BuiltinRequestState;
use crate::{
    FilesystemCapabilities, IniRegistry, MysqlState, ObjectRef, OutputBuffer, PHP_E_DEPRECATED,
    PHP_E_NOTICE, PHP_E_WARNING, PcreCache, PhpArray, PhpDiagnosticChannel,
    PhpDiagnosticDisplayOptions, PostgresState, ReferenceCell, ResourceTable, RuntimeDiagnostic,
    RuntimeHttpResponseState, RuntimeSeverity, SessionLoadCallback, SessionState, UploadRegistry,
    Value, datetime, emit_php_diagnostic, source_span::RuntimeSourceSpan,
};
use curl::easy::{Handler, WriteError};
use curl::multi::{Easy2Handle, Multi};
use imap::{ClientBuilder, ConnectionMode};
use ldap3::result::LdapError;
use ldap3::{LdapConn, Scope, SearchEntry};
use ssh2::{HashType, Session as Ssh2BackendSession, Sftp as Ssh2BackendSftp};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::io::{Cursor, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, SystemTime};
use suppaftp::types::{FileType, FormatControl, Response};
use suppaftp::{FtpStream, Mode, Status};

mod legacy_extension_state;
mod service_views;
pub use legacy_extension_state::*;
pub(in crate::builtins) use service_views::{
    CurlBuiltinServices, JsonBuiltinServices, PcreBuiltinServices, PcreCallbackServiceAccess,
    PcreCallbackServices, PcreServiceAccess,
};

pub(in crate::builtins) struct BuiltinIoContext<'a> {
    output: &'a mut OutputBuffer,
    php_input: Arc<[u8]>,
    diagnostic_display: PhpDiagnosticDisplayOptions,
    diagnostics: Vec<RuntimeDiagnostic>,
}

impl<'a> BuiltinIoContext<'a> {
    fn new(output: &'a mut OutputBuffer) -> Self {
        Self {
            output,
            php_input: Arc::from([]),
            diagnostic_display: PhpDiagnosticDisplayOptions::default(),
            diagnostics: Vec::new(),
        }
    }

    fn php_warning(
        &mut self,
        id: impl Into<String>,
        message: impl Into<String>,
        source_span: RuntimeSourceSpan,
    ) {
        let diagnostic = RuntimeDiagnostic::new(
            id,
            RuntimeSeverity::Warning,
            message.into(),
            source_span,
            Vec::new(),
            Some(crate::PhpReferenceClassification::Warning),
        );
        emit_php_diagnostic(
            self.output,
            &diagnostic,
            PhpDiagnosticChannel::Warning,
            PHP_E_WARNING,
            self.diagnostic_display,
        );
        self.diagnostics.push(diagnostic);
    }
}

pub(in crate::builtins) struct BuiltinFilesystemContext<'a> {
    cwd: PathBuf,
    cwd_slot: Option<&'a mut PathBuf>,
    include_path: Arc<Vec<PathBuf>>,
    filesystem: FilesystemCapabilities,
    resources: Option<&'a mut ResourceTable>,
    filesystem_state: FilesystemRuntimeState,
    filesystem_state_slot: Option<&'a mut FilesystemRuntimeState>,
    stream_context_state: StreamContextState,
    stream_context_state_slot: Option<&'a mut StreamContextState>,
}

impl<'a> BuiltinFilesystemContext<'a> {
    fn new(
        cwd: impl Into<PathBuf>,
        filesystem: FilesystemCapabilities,
        resources: Option<&'a mut ResourceTable>,
    ) -> Self {
        Self {
            cwd: cwd.into(),
            cwd_slot: None,
            include_path: Arc::new(vec![PathBuf::from(".")]),
            filesystem,
            resources,
            filesystem_state: FilesystemRuntimeState::default(),
            filesystem_state_slot: None,
            stream_context_state: StreamContextState::default(),
            stream_context_state_slot: None,
        }
    }
}

#[derive(Default)]
pub(in crate::builtins) struct BuiltinHttpContext<'a> {
    http_response: RuntimeHttpResponseState,
    http_response_state: Option<&'a mut RuntimeHttpResponseState>,
    filter_inputs: Rc<BTreeMap<i64, crate::PhpArray>>,
    upload_registry: Option<&'a mut UploadRegistry>,
}

pub(in crate::builtins) struct BuiltinExtensionState<'a> {
    bcmath_scale: usize,
    strtok_state: Option<&'a mut StrtokState>,
    iconv_state: IconvEncodingState,
    iconv_state_slot: Option<&'a mut IconvEncodingState>,
    apcu_state: ApcuState,
    apcu_state_slot: Option<&'a mut ApcuState>,
    opcache_state: OpcacheState,
    opcache_state_slot: Option<&'a mut OpcacheState>,
    soap_state: SoapState,
    soap_state_slot: Option<&'a mut SoapState>,
    openssl_error_state: OpenSslErrorState,
    openssl_error_state_slot: Option<&'a mut OpenSslErrorState>,
    gettext_state: GettextState,
    gettext_state_slot: Option<&'a mut GettextState>,
    shmop_state: ShmopState,
    shmop_state_slot: Option<&'a mut ShmopState>,
    readline_state: ReadlineState,
    readline_state_slot: Option<&'a mut ReadlineState>,
    sysvmsg_state: SysvMessageQueueState,
    sysvmsg_state_slot: Option<&'a mut SysvMessageQueueState>,
    sysvsem_state: SysvSemaphoreState,
    sysvsem_state_slot: Option<&'a mut SysvSemaphoreState>,
    sysvshm_state: SysvSharedMemoryState,
    sysvshm_state_slot: Option<&'a mut SysvSharedMemoryState>,
    pcntl_state: PcntlState,
    pcntl_state_slot: Option<&'a mut PcntlState>,
    ftp_state: FtpState,
    ftp_state_slot: Option<&'a mut FtpState>,
    imap_state: ImapState,
    imap_state_slot: Option<&'a mut ImapState>,
    ldap_state: LdapState,
    ldap_state_slot: Option<&'a mut LdapState>,
    ssh2_state: Ssh2State,
    ssh2_state_slot: Option<&'a mut Ssh2State>,
    socket_state: SocketState,
    socket_state_slot: Option<&'a mut SocketState>,
    posix_last_error: i32,
    mb_internal_encoding: String,
    mb_internal_encoding_slot: Option<&'a mut String>,
    mb_substitute_character: MbSubstituteCharacter,
    mb_substitute_character_slot: Option<&'a mut MbSubstituteCharacter>,
    mysql_state: Option<&'a mut MysqlState>,
    postgres_state: Option<&'a mut PostgresState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MbSubstituteCharacter {
    Codepoint(i64),
    Mode(&'static str),
}

impl Default for MbSubstituteCharacter {
    fn default() -> Self {
        Self::Codepoint(63)
    }
}

impl<'a> Default for BuiltinExtensionState<'a> {
    fn default() -> Self {
        Self {
            bcmath_scale: 0,
            strtok_state: None,
            iconv_state: IconvEncodingState::default(),
            iconv_state_slot: None,
            apcu_state: ApcuState::default(),
            apcu_state_slot: None,
            opcache_state: OpcacheState::default(),
            opcache_state_slot: None,
            soap_state: SoapState::default(),
            soap_state_slot: None,
            openssl_error_state: OpenSslErrorState::default(),
            openssl_error_state_slot: None,
            gettext_state: GettextState::default(),
            gettext_state_slot: None,
            shmop_state: ShmopState::default(),
            shmop_state_slot: None,
            readline_state: ReadlineState::default(),
            readline_state_slot: None,
            sysvmsg_state: SysvMessageQueueState::default(),
            sysvmsg_state_slot: None,
            sysvsem_state: SysvSemaphoreState::default(),
            sysvsem_state_slot: None,
            sysvshm_state: SysvSharedMemoryState::default(),
            sysvshm_state_slot: None,
            pcntl_state: PcntlState::default(),
            pcntl_state_slot: None,
            ftp_state: FtpState::default(),
            ftp_state_slot: None,
            imap_state: ImapState::default(),
            imap_state_slot: None,
            ldap_state: LdapState::default(),
            ldap_state_slot: None,
            ssh2_state: Ssh2State::default(),
            ssh2_state_slot: None,
            socket_state: SocketState::default(),
            socket_state_slot: None,
            posix_last_error: 0,
            mb_internal_encoding: "UTF-8".to_owned(),
            mb_internal_encoding_slot: None,
            mb_substitute_character: MbSubstituteCharacter::Codepoint(63),
            mb_substitute_character_slot: None,
            mysql_state: None,
            postgres_state: None,
        }
    }
}

#[derive(Default)]
pub(in crate::builtins) struct BuiltinSessionContext<'a> {
    session_state: Option<&'a mut SessionState>,
    session_global: Option<ReferenceCell>,
    session_loader: Option<&'a SessionLoadCallback>,
}

enum BuiltinRequestStateAccess<'a> {
    Owned(BuiltinRequestState),
    Borrowed(&'a mut BuiltinRequestState),
}

impl BuiltinRequestStateAccess<'_> {
    fn get(&self) -> &BuiltinRequestState {
        match self {
            Self::Owned(state) => state,
            Self::Borrowed(state) => state,
        }
    }

    fn get_mut(&mut self) -> &mut BuiltinRequestState {
        match self {
            Self::Owned(state) => state,
            Self::Borrowed(state) => state,
        }
    }
}

/// Mutable runtime services available to internal builtins.
pub struct BuiltinContext<'a> {
    io: BuiltinIoContext<'a>,
    filesystem: BuiltinFilesystemContext<'a>,
    http: BuiltinHttpContext<'a>,
    request_state: BuiltinRequestStateAccess<'a>,
    extensions: BuiltinExtensionState<'a>,
    sessions: BuiltinSessionContext<'a>,
    ini: IniRegistry,
    ini_slot: Option<&'a mut IniRegistry>,
    default_timezone: String,
    default_timezone_slot: Option<&'a mut String>,
    env: Arc<Vec<(String, String)>>,
    network_requests_enabled: bool,
}

impl<'a> BuiltinContext<'a> {
    /// Creates a runtime context backed by the VM output buffer.
    #[must_use]
    pub fn new(output: &'a mut OutputBuffer) -> Self {
        Self {
            io: BuiltinIoContext::new(output),
            filesystem: BuiltinFilesystemContext::new(
                PathBuf::from("."),
                FilesystemCapabilities::none(),
                None,
            ),
            http: BuiltinHttpContext::default(),
            request_state: BuiltinRequestStateAccess::Owned(BuiltinRequestState::new()),
            extensions: BuiltinExtensionState::default(),
            sessions: BuiltinSessionContext::default(),
            ini: IniRegistry::default(),
            ini_slot: None,
            default_timezone: datetime::DEFAULT_TIMEZONE.to_string(),
            default_timezone_slot: None,
            env: Arc::new(Vec::new()),
            network_requests_enabled: false,
        }
    }

    /// Creates a context borrowing an existing request-state owner.
    #[must_use]
    pub fn new_with_request_state(
        output: &'a mut OutputBuffer,
        request_state: &'a mut BuiltinRequestState,
    ) -> Self {
        Self::with_runtime_request_state(
            output,
            PathBuf::from("."),
            FilesystemCapabilities::none(),
            None,
            request_state,
        )
    }

    /// Creates a runtime context with deterministic host capability policy.
    #[must_use]
    pub fn with_runtime(
        output: &'a mut OutputBuffer,
        cwd: impl Into<PathBuf>,
        filesystem: FilesystemCapabilities,
        resources: Option<&'a mut ResourceTable>,
    ) -> Self {
        Self {
            io: BuiltinIoContext::new(output),
            filesystem: BuiltinFilesystemContext::new(cwd, filesystem, resources),
            http: BuiltinHttpContext::default(),
            request_state: BuiltinRequestStateAccess::Owned(BuiltinRequestState::new()),
            extensions: BuiltinExtensionState::default(),
            sessions: BuiltinSessionContext::default(),
            ini: IniRegistry::default(),
            ini_slot: None,
            default_timezone: datetime::DEFAULT_TIMEZONE.to_string(),
            default_timezone_slot: None,
            env: Arc::new(Vec::new()),
            network_requests_enabled: false,
        }
    }

    /// Creates a runtime context borrowing the request's sole extension-state owner.
    #[must_use]
    pub fn with_runtime_request_state(
        output: &'a mut OutputBuffer,
        cwd: impl Into<PathBuf>,
        filesystem: FilesystemCapabilities,
        resources: Option<&'a mut ResourceTable>,
        request_state: &'a mut BuiltinRequestState,
    ) -> Self {
        Self {
            io: BuiltinIoContext::new(output),
            filesystem: BuiltinFilesystemContext::new(cwd, filesystem, resources),
            http: BuiltinHttpContext::default(),
            request_state: BuiltinRequestStateAccess::Borrowed(request_state),
            extensions: BuiltinExtensionState::default(),
            sessions: BuiltinSessionContext::default(),
            ini: IniRegistry::default(),
            ini_slot: None,
            default_timezone: datetime::DEFAULT_TIMEZONE.to_string(),
            default_timezone_slot: None,
            env: Arc::new(Vec::new()),
            network_requests_enabled: false,
        }
    }

    /// Sets deterministic request-local environment entries. Pre-sorted
    /// tables (the common case: the VM keeps its request env sorted) are
    /// shared without copying.
    pub fn set_env_entries(&mut self, env: Arc<Vec<(String, String)>>) {
        if env.windows(2).all(|pair| {
            pair[0].0 <= pair[1].0 && !(pair[0].0 == pair[1].0 && pair[0].1 > pair[1].1)
        }) {
            self.env = env;
            return;
        }
        let mut owned = env.as_ref().clone();
        owned.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
        self.env = Arc::new(owned);
    }

    /// Reads a deterministic request-local environment value.
    #[must_use]
    pub fn env_value(&self, name: &str) -> Option<&str> {
        self.env
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// Allows request-local network builtins without reading process-global env.
    pub fn set_network_requests_enabled(&mut self, enabled: bool) {
        self.network_requests_enabled = enabled;
    }

    /// Returns whether request-local network builtins are explicitly enabled.
    #[must_use]
    pub const fn network_requests_enabled(&self) -> bool {
        self.network_requests_enabled
    }

    /// Returns the output buffer.
    pub fn output(&mut self) -> &mut OutputBuffer {
        self.io.output
    }

    /// Sets request-local warning/error output controls.
    pub fn set_diagnostic_display(&mut self, options: PhpDiagnosticDisplayOptions) {
        self.io.diagnostic_display = options;
    }

    /// Emits a PHP display_errors-style warning into stdout and records a
    /// structured diagnostic for VM/report consumers.
    pub fn php_warning(
        &mut self,
        id: impl Into<String>,
        message: impl Into<String>,
        source_span: RuntimeSourceSpan,
    ) {
        self.io.php_warning(id, message, source_span);
    }

    /// Emits a PHP display_errors-style notice into stdout and records a
    /// structured diagnostic for VM/report consumers.
    pub fn php_notice(
        &mut self,
        id: impl Into<String>,
        message: impl Into<String>,
        source_span: RuntimeSourceSpan,
    ) {
        let message = message.into();
        let diagnostic = RuntimeDiagnostic::new(
            id,
            RuntimeSeverity::Notice,
            message,
            source_span,
            Vec::new(),
            Some(crate::PhpReferenceClassification::Notice),
        );
        emit_php_diagnostic(
            self.io.output,
            &diagnostic,
            PhpDiagnosticChannel::Notice,
            PHP_E_NOTICE,
            self.io.diagnostic_display,
        );
        self.io.diagnostics.push(diagnostic);
    }

    /// Emits a PHP display_errors-style deprecation into stdout and records a
    /// structured diagnostic for VM/report consumers.
    pub fn php_deprecation(
        &mut self,
        id: impl Into<String>,
        message: impl Into<String>,
        source_span: RuntimeSourceSpan,
    ) {
        let message = message.into();
        let diagnostic = RuntimeDiagnostic::new(
            id,
            RuntimeSeverity::Deprecation,
            message,
            source_span,
            Vec::new(),
            Some(crate::PhpReferenceClassification::Deprecation),
        );
        emit_php_diagnostic(
            self.io.output,
            &diagnostic,
            PhpDiagnosticChannel::Deprecated,
            PHP_E_DEPRECATED,
            self.io.diagnostic_display,
        );
        self.io.diagnostics.push(diagnostic);
    }

    /// Records a structured diagnostic without emitting PHP-visible output.
    pub fn record_diagnostic(&mut self, diagnostic: RuntimeDiagnostic) {
        self.io.diagnostics.push(diagnostic);
    }

    /// Drains structured diagnostics emitted by builtins.
    pub fn take_diagnostics(&mut self) -> Vec<RuntimeDiagnostic> {
        std::mem::take(&mut self.io.diagnostics)
    }

    /// Current working directory for path and filesystem builtins.
    #[must_use]
    pub fn cwd(&self) -> &Path {
        self.filesystem
            .cwd_slot
            .as_deref()
            .unwrap_or(&self.filesystem.cwd)
    }

    /// Updates the request-local current working directory for filesystem builtins.
    pub fn set_cwd(&mut self, cwd: impl Into<PathBuf>) {
        let cwd = cwd.into();
        match self.filesystem.cwd_slot.as_deref_mut() {
            Some(slot) => *slot = cwd,
            None => self.filesystem.cwd = cwd,
        }
    }

    /// Borrows the VM-owned request current working directory.
    pub fn set_cwd_state(&mut self, cwd: &'a mut PathBuf) {
        self.filesystem.cwd_slot = Some(cwd);
    }

    /// Include path entries used by stream include-path resolution.
    #[must_use]
    pub fn include_path(&self) -> &[PathBuf] {
        self.filesystem.include_path.as_slice()
    }

    /// Sets request-local include path entries.
    pub fn set_include_path(&mut self, include_path: Vec<PathBuf>) {
        self.filesystem.include_path = Arc::new(include_path);
    }

    /// Shares a request-scoped parsed include path.
    pub fn set_include_path_shared(&mut self, include_path: Arc<Vec<PathBuf>>) {
        self.filesystem.include_path = include_path;
    }

    /// Reads a request-local INI option visible to standard-library builtins.
    #[must_use]
    pub fn ini_get(&self, name: &str) -> Option<&str> {
        self.ini_registry().get(name)
    }

    /// Sets request-local INI options visible to standard-library builtins.
    pub fn set_ini_registry(&mut self, ini: IniRegistry) {
        self.ini = ini;
        self.ini_slot = None;
    }

    /// Borrows the VM-owned request INI registry.
    pub fn set_ini_registry_state(&mut self, ini: &'a mut IniRegistry) {
        self.ini_slot = Some(ini);
    }

    /// Returns request-local INI options visible to standard-library builtins.
    #[must_use]
    pub fn ini_registry(&self) -> &IniRegistry {
        self.ini_slot.as_deref().unwrap_or(&self.ini)
    }

    /// Updates a request-local INI option visible to standard-library builtins.
    pub fn ini_set(&mut self, name: &str, value: impl Into<String>) -> Option<String> {
        match self.ini_slot.as_deref_mut() {
            Some(ini) => ini.set(name, value),
            None => self.ini.set(name, value),
        }
    }

    /// Current request-local default timezone.
    #[must_use]
    pub fn default_timezone(&self) -> &str {
        self.default_timezone_slot
            .as_deref()
            .map(String::as_str)
            .unwrap_or(&self.default_timezone)
    }

    /// Updates the request-local default timezone.
    pub fn set_default_timezone(&mut self, identifier: impl Into<String>) {
        let identifier = identifier.into();
        match self.default_timezone_slot.as_deref_mut() {
            Some(timezone) => *timezone = identifier,
            None => self.default_timezone = identifier,
        }
    }

    /// Borrows the VM-owned request timezone.
    pub fn set_default_timezone_state(&mut self, timezone: &'a mut String) {
        self.default_timezone_slot = Some(timezone);
    }

    /// Filesystem capabilities for path and filesystem builtins.
    #[must_use]
    pub fn filesystem_capabilities(&self) -> &FilesystemCapabilities {
        &self.filesystem.filesystem
    }

    /// Sets deterministic bytes exposed through `php://input`.
    pub fn set_php_input(&mut self, input: impl Into<Arc<[u8]>>) {
        self.io.php_input = input.into();
    }

    /// Deterministic bytes exposed through `php://input`.
    #[must_use]
    pub fn php_input(&self) -> &[u8] {
        &self.io.php_input
    }

    /// Request-local resource table for stream builtins.
    pub fn resources(&mut self) -> Option<&mut ResourceTable> {
        self.filesystem.resources.as_deref_mut()
    }

    /// Sets request-local HTTP response state.
    pub fn set_http_response_state(&mut self, state: &'a mut RuntimeHttpResponseState) {
        self.http.http_response_state = Some(state);
    }

    /// Current request-local HTTP response state.
    #[must_use]
    pub fn http_response(&self) -> &RuntimeHttpResponseState {
        self.http
            .http_response_state
            .as_deref()
            .unwrap_or(&self.http.http_response)
    }

    /// Mutable request-local HTTP response state.
    pub fn http_response_mut(&mut self) -> &mut RuntimeHttpResponseState {
        match self.http.http_response_state.as_deref_mut() {
            Some(state) => state,
            None => &mut self.http.http_response,
        }
    }

    /// Sets a deterministic request-input array for `filter_input`.
    pub fn set_filter_input_array(&mut self, source: i64, array: crate::PhpArray) {
        Rc::make_mut(&mut self.http.filter_inputs).insert(source, array);
    }

    /// Shares request-input arrays materialized once during request setup.
    pub fn set_filter_input_arrays_shared(&mut self, arrays: Rc<BTreeMap<i64, crate::PhpArray>>) {
        self.http.filter_inputs = arrays;
    }

    /// Looks up a top-level request-input value for `filter_input`.
    #[must_use]
    pub fn filter_input_value(&self, source: i64, name: &str) -> Option<Value> {
        self.http.filter_inputs.get(&source).and_then(|array| {
            array
                .get(&crate::ArrayKey::String(crate::PhpString::from_test_str(
                    name,
                )))
                .cloned()
        })
    }

    /// Returns a request-input array snapshot for `filter_input_array`.
    #[must_use]
    pub fn filter_input_array(&self, source: i64) -> Option<crate::PhpArray> {
        self.http.filter_inputs.get(&source).cloned()
    }

    /// Sets request-local upload registry state.
    pub fn set_upload_registry(&mut self, registry: &'a mut UploadRegistry) {
        self.http.upload_registry = Some(registry);
    }

    /// Current request-local upload registry state.
    pub fn upload_registry(&self) -> Option<&UploadRegistry> {
        self.http.upload_registry.as_deref()
    }

    /// Mutable request-local upload registry state.
    pub fn upload_registry_mut(&mut self) -> Option<&mut UploadRegistry> {
        self.http.upload_registry.as_deref_mut()
    }

    /// Sets request-local `strtok` state.
    pub fn set_strtok_state(&mut self, state: &'a mut StrtokState) {
        self.extensions.strtok_state = Some(state);
    }

    /// Returns request-local `strtok` state.
    pub fn strtok_state(&mut self) -> Option<&mut StrtokState> {
        self.extensions.strtok_state.as_deref_mut()
    }

    /// Sets request-local iconv encoding state.
    pub fn set_iconv_state(&mut self, state: &'a mut IconvEncodingState) {
        self.extensions.iconv_state_slot = Some(state);
    }

    /// Mutable request-local iconv encoding state.
    pub fn iconv_state(&mut self) -> &mut IconvEncodingState {
        match self.extensions.iconv_state_slot.as_deref_mut() {
            Some(state) => state,
            None => &mut self.extensions.iconv_state,
        }
    }

    /// Sets an APCu state handle. Default handles share process-local storage.
    pub fn set_apcu_state(&mut self, state: &'a mut ApcuState) {
        self.extensions.apcu_state_slot = Some(state);
    }

    /// Mutable APCu state handle.
    pub fn apcu_state(&mut self) -> &mut ApcuState {
        match self.extensions.apcu_state_slot.as_deref_mut() {
            Some(state) => state,
            None => &mut self.extensions.apcu_state,
        }
    }

    /// Sets request-local OPcache facade state.
    pub fn set_opcache_state(&mut self, state: &'a mut OpcacheState) {
        self.extensions.opcache_state_slot = Some(state);
    }

    /// Mutable request-local OPcache facade state.
    pub fn opcache_state(&mut self) -> &mut OpcacheState {
        match self.extensions.opcache_state_slot.as_deref_mut() {
            Some(state) => state,
            None => &mut self.extensions.opcache_state,
        }
    }

    /// Sets request-local SOAP facade state.
    pub fn set_soap_state(&mut self, state: &'a mut SoapState) {
        self.extensions.soap_state_slot = Some(state);
    }

    /// Mutable request-local SOAP facade state.
    pub fn soap_state(&mut self) -> &mut SoapState {
        match self.extensions.soap_state_slot.as_deref_mut() {
            Some(state) => state,
            None => &mut self.extensions.soap_state,
        }
    }

    /// Sets request-local OpenSSL error queue state.
    pub fn set_openssl_error_state(&mut self, state: &'a mut OpenSslErrorState) {
        self.extensions.openssl_error_state_slot = Some(state);
    }

    /// Appends an OpenSSL error string to the request-local queue.
    pub fn push_openssl_error(&mut self, error: impl Into<String>) {
        match self.extensions.openssl_error_state_slot.as_deref_mut() {
            Some(state) => state.push(error),
            None => self.extensions.openssl_error_state.push(error),
        }
    }

    /// Returns and removes the oldest OpenSSL error string.
    pub fn pop_openssl_error(&mut self) -> Option<String> {
        match self.extensions.openssl_error_state_slot.as_deref_mut() {
            Some(state) => state.pop(),
            None => self.extensions.openssl_error_state.pop(),
        }
    }

    /// Uses VM-owned gettext state for request-local gettext builtins.
    pub fn set_gettext_state(&mut self, state: &'a mut GettextState) {
        self.extensions.gettext_state_slot = Some(state);
    }

    /// Returns request-local gettext state.
    pub fn gettext_state(&mut self) -> &mut GettextState {
        match self.extensions.gettext_state_slot.as_deref_mut() {
            Some(state) => state,
            None => &mut self.extensions.gettext_state,
        }
    }

    /// Returns immutable request-local gettext state.
    #[must_use]
    pub fn gettext_state_ref(&self) -> &GettextState {
        self.extensions
            .gettext_state_slot
            .as_deref()
            .unwrap_or(&self.extensions.gettext_state)
    }

    /// Uses VM-owned shmop state for request-local shmop builtins.
    pub fn set_shmop_state(&mut self, state: &'a mut ShmopState) {
        self.extensions.shmop_state_slot = Some(state);
    }

    /// Returns request-local shmop state.
    pub fn shmop_state(&mut self) -> &mut ShmopState {
        match self.extensions.shmop_state_slot.as_deref_mut() {
            Some(state) => state,
            None => &mut self.extensions.shmop_state,
        }
    }

    /// Uses VM-owned readline state for request-local readline builtins.
    pub fn set_readline_state(&mut self, state: &'a mut ReadlineState) {
        self.extensions.readline_state_slot = Some(state);
    }

    /// Returns request-local readline state.
    pub fn readline_state(&mut self) -> &mut ReadlineState {
        match self.extensions.readline_state_slot.as_deref_mut() {
            Some(state) => state,
            None => &mut self.extensions.readline_state,
        }
    }

    /// Uses VM-owned System V message queue state for request-local sysvmsg builtins.
    pub fn set_sysvmsg_state(&mut self, state: &'a mut SysvMessageQueueState) {
        self.extensions.sysvmsg_state_slot = Some(state);
    }

    /// Returns request-local System V message queue state.
    pub fn sysvmsg_state(&mut self) -> &mut SysvMessageQueueState {
        match self.extensions.sysvmsg_state_slot.as_deref_mut() {
            Some(state) => state,
            None => &mut self.extensions.sysvmsg_state,
        }
    }

    /// Uses VM-owned System V semaphore state for request-local sysvsem builtins.
    pub fn set_sysvsem_state(&mut self, state: &'a mut SysvSemaphoreState) {
        self.extensions.sysvsem_state_slot = Some(state);
    }

    /// Returns request-local System V semaphore state.
    pub fn sysvsem_state(&mut self) -> &mut SysvSemaphoreState {
        match self.extensions.sysvsem_state_slot.as_deref_mut() {
            Some(state) => state,
            None => &mut self.extensions.sysvsem_state,
        }
    }

    /// Uses VM-owned System V shared variable state for request-local sysvshm builtins.
    pub fn set_sysvshm_state(&mut self, state: &'a mut SysvSharedMemoryState) {
        self.extensions.sysvshm_state_slot = Some(state);
    }

    /// Returns request-local System V shared variable state.
    pub fn sysvshm_state(&mut self) -> &mut SysvSharedMemoryState {
        match self.extensions.sysvshm_state_slot.as_deref_mut() {
            Some(state) => state,
            None => &mut self.extensions.sysvshm_state,
        }
    }

    /// Uses VM-owned PCNTL state for request-local PCNTL builtins.
    pub fn set_pcntl_state(&mut self, state: &'a mut PcntlState) {
        self.extensions.pcntl_state_slot = Some(state);
    }

    /// Returns request-local PCNTL state.
    pub fn pcntl_state(&mut self) -> &mut PcntlState {
        match self.extensions.pcntl_state_slot.as_deref_mut() {
            Some(state) => state,
            None => &mut self.extensions.pcntl_state,
        }
    }

    /// Uses VM-owned FTP state for request-local FTP builtins.
    pub fn set_ftp_state(&mut self, state: &'a mut FtpState) {
        self.extensions.ftp_state_slot = Some(state);
    }

    /// Returns request-local FTP state.
    pub fn ftp_state(&mut self) -> &mut FtpState {
        match self.extensions.ftp_state_slot.as_deref_mut() {
            Some(state) => state,
            None => &mut self.extensions.ftp_state,
        }
    }

    /// Uses VM-owned IMAP state for request-local IMAP builtins.
    pub fn set_imap_state(&mut self, state: &'a mut ImapState) {
        self.extensions.imap_state_slot = Some(state);
    }

    /// Returns request-local IMAP state.
    pub fn imap_state(&mut self) -> &mut ImapState {
        match self.extensions.imap_state_slot.as_deref_mut() {
            Some(state) => state,
            None => &mut self.extensions.imap_state,
        }
    }

    /// Uses VM-owned LDAP state for request-local LDAP builtins.
    pub fn set_ldap_state(&mut self, state: &'a mut LdapState) {
        self.extensions.ldap_state_slot = Some(state);
    }

    /// Returns request-local LDAP state.
    pub fn ldap_state(&mut self) -> &mut LdapState {
        match self.extensions.ldap_state_slot.as_deref_mut() {
            Some(state) => state,
            None => &mut self.extensions.ldap_state,
        }
    }

    /// Uses VM-owned SSH2 state for request-local SSH2 builtins.
    pub fn set_ssh2_state(&mut self, state: &'a mut Ssh2State) {
        self.extensions.ssh2_state_slot = Some(state);
    }

    /// Returns request-local SSH2 state.
    pub fn ssh2_state(&mut self) -> &mut Ssh2State {
        match self.extensions.ssh2_state_slot.as_deref_mut() {
            Some(state) => state,
            None => &mut self.extensions.ssh2_state,
        }
    }

    /// Uses VM-owned sockets state for request-local socket builtins.
    pub fn set_socket_state(&mut self, state: &'a mut SocketState) {
        self.extensions.socket_state_slot = Some(state);
    }

    /// Returns request-local sockets state.
    pub fn socket_state(&mut self) -> &mut SocketState {
        match self.extensions.socket_state_slot.as_deref_mut() {
            Some(state) => state,
            None => &mut self.extensions.socket_state,
        }
    }

    /// Updates the request-local POSIX errno value.
    pub fn set_posix_last_error(&mut self, error: i32) {
        self.extensions.posix_last_error = error;
    }

    /// Returns the request-local POSIX errno value.
    #[must_use]
    pub const fn posix_last_error(&self) -> i32 {
        self.extensions.posix_last_error
    }

    /// Current request-local bcmath default scale.
    #[must_use]
    pub const fn bcmath_scale(&self) -> usize {
        self.extensions.bcmath_scale
    }

    /// Updates the request-local bcmath default scale and returns the previous value.
    pub fn set_bcmath_scale(&mut self, scale: usize) -> usize {
        let previous = self.extensions.bcmath_scale;
        self.extensions.bcmath_scale = scale;
        previous
    }

    /// Sets request-local filesystem builtin state.
    pub fn set_filesystem_state(&mut self, state: &'a mut FilesystemRuntimeState) {
        self.filesystem.filesystem_state_slot = Some(state);
    }

    /// Mutable request-local filesystem builtin state.
    pub fn filesystem_state(&mut self) -> &mut FilesystemRuntimeState {
        match self.filesystem.filesystem_state_slot.as_deref_mut() {
            Some(state) => state,
            None => &mut self.filesystem.filesystem_state,
        }
    }

    /// Sets request-local stream context default state.
    pub fn set_stream_context_state(&mut self, state: &'a mut StreamContextState) {
        self.filesystem.stream_context_state_slot = Some(state);
    }

    /// Mutable request-local stream context default state.
    pub fn stream_context_state(&mut self) -> &mut StreamContextState {
        match self.filesystem.stream_context_state_slot.as_deref_mut() {
            Some(state) => state,
            None => &mut self.filesystem.stream_context_state,
        }
    }

    /// Current request-local mbstring internal encoding.
    #[must_use]
    pub fn mb_internal_encoding(&self) -> &str {
        self.extensions
            .mb_internal_encoding_slot
            .as_deref()
            .map(String::as_str)
            .unwrap_or(&self.extensions.mb_internal_encoding)
    }

    /// Updates the request-local mbstring internal encoding.
    pub fn set_mb_internal_encoding(&mut self, encoding: impl Into<String>) {
        let encoding = encoding.into();
        match self.extensions.mb_internal_encoding_slot.as_deref_mut() {
            Some(slot) => *slot = encoding,
            None => self.extensions.mb_internal_encoding = encoding,
        }
    }

    /// Borrows the VM-owned request mbstring encoding.
    pub fn set_mb_internal_encoding_state(&mut self, encoding: &'a mut String) {
        self.extensions.mb_internal_encoding_slot = Some(encoding);
    }

    /// Current request-local mbstring substitute-character mode.
    #[must_use]
    pub fn mb_substitute_character(&self) -> &MbSubstituteCharacter {
        self.extensions
            .mb_substitute_character_slot
            .as_deref()
            .unwrap_or(&self.extensions.mb_substitute_character)
    }

    /// Updates the request-local mbstring substitute-character mode.
    pub fn set_mb_substitute_character(&mut self, substitute: MbSubstituteCharacter) {
        match self.extensions.mb_substitute_character_slot.as_deref_mut() {
            Some(slot) => *slot = substitute,
            None => self.extensions.mb_substitute_character = substitute,
        }
    }

    /// Borrows the VM-owned request mbstring substitute-character state.
    pub fn set_mb_substitute_character_state(&mut self, substitute: &'a mut MbSubstituteCharacter) {
        self.extensions.mb_substitute_character_slot = Some(substitute);
    }

    /// Sets request-local session state and the live `$_SESSION` global slot.
    pub fn set_session_state(
        &mut self,
        state: &'a mut SessionState,
        session_global: ReferenceCell,
    ) {
        self.sessions.session_state = Some(state);
        self.sessions.session_global = Some(session_global);
    }

    /// Sets the request-local transport callback for lazy session data loading.
    pub fn set_session_loader(&mut self, loader: Option<&'a SessionLoadCallback>) {
        self.sessions.session_loader = loader;
    }

    /// Request-local session state.
    pub fn session_state(&mut self) -> Option<&mut SessionState> {
        self.sessions.session_state.as_deref_mut()
    }

    /// Loads pending session data from the transport layer when needed.
    pub fn load_pending_session_data(&mut self) -> Result<(), String> {
        let Some(state) = self.sessions.session_state.as_deref_mut() else {
            return Ok(());
        };
        if !state.needs_lazy_load() {
            return Ok(());
        }
        let id = state.id().to_owned();
        let Some(loader) = self.sessions.session_loader else {
            return Err("session loader is unavailable".to_string());
        };
        let data = loader.load(&id)?;
        state.load_data(data);
        Ok(())
    }

    /// Sets request-local MySQL/MariaDB extension state.
    pub fn set_mysql_state(&mut self, state: &'a mut MysqlState) {
        self.extensions.mysql_state = Some(state);
    }

    /// Returns request-local MySQL/MariaDB extension state.
    pub fn mysql_state(&mut self) -> Option<&mut MysqlState> {
        self.extensions.mysql_state.as_deref_mut()
    }

    /// Sets request-local PostgreSQL extension state.
    pub fn set_postgres_state(&mut self, state: &'a mut PostgresState) {
        self.extensions.postgres_state = Some(state);
    }

    /// Returns request-local PostgreSQL extension state.
    pub fn postgres_state(&mut self) -> Option<&mut PostgresState> {
        self.extensions.postgres_state.as_deref_mut()
    }

    /// Writes the current session data into the live `$_SESSION` global.
    pub fn sync_session_global_from_state(&mut self) {
        let Some(data) = self
            .sessions
            .session_state
            .as_ref()
            .map(|state| state.data_value())
        else {
            return;
        };
        if let Some(global) = &self.sessions.session_global {
            global.set(data);
        }
    }

    /// Captures the live `$_SESSION` global back into request-local session state.
    pub fn sync_session_state_from_global(&mut self) {
        let Some(global) = &self.sessions.session_global else {
            return;
        };
        let Value::Array(array) = global.get() else {
            return;
        };
        if let Some(state) = self.sessions.session_state.as_deref_mut() {
            state.set_data(array);
        }
    }
}

pub(in crate::builtins) const fn json_error_message(code: i64) -> &'static str {
    match code {
        JSON_ERROR_NONE => "No error",
        JSON_ERROR_DEPTH => "Maximum stack depth exceeded",
        JSON_ERROR_STATE_MISMATCH => "State mismatch (invalid or malformed JSON)",
        JSON_ERROR_CTRL_CHAR => "Control character error, possibly incorrectly encoded",
        JSON_ERROR_SYNTAX => "Syntax error",
        JSON_ERROR_UTF8 => "Malformed UTF-8 characters, possibly incorrectly encoded",
        JSON_ERROR_RECURSION => "Recursion detected",
        JSON_ERROR_INF_OR_NAN => "Inf and NaN cannot be JSON encoded",
        JSON_ERROR_UNSUPPORTED_TYPE => "Type is not supported",
        JSON_ERROR_INVALID_PROPERTY_NAME => "The decoded property name is invalid",
        JSON_ERROR_UTF16 => "Single unpaired UTF-16 surrogate in unicode escape",
        JSON_ERROR_NON_BACKED_ENUM => "Non-backed enums have no default serialization",
        _ => "Unknown error",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BuiltinContext, JSON_ERROR_NONE, JSON_ERROR_SYNTAX, PcreServiceAccess, RuntimeSourceSpan,
        StrtokState, json_error_message,
    };
    use crate::api::{
        ArrayKey, BuiltinRequestState, OutputBuffer, PhpArray, PhpString, ReferenceCell,
        RuntimeHttpResponseState, RuntimeUploadedFile, SessionState, UploadRegistry, Value,
    };
    use crate::pcre;
    use std::path::PathBuf;

    #[test]
    fn json_last_error_state_updates_and_reads_from_extension_state() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);

        let mut services = context.json_services();
        services.set_json_last_error(JSON_ERROR_SYNTAX);
        assert_eq!(
            services.json_last_error(),
            (JSON_ERROR_SYNTAX, json_error_message(JSON_ERROR_SYNTAX))
        );

        services.set_json_last_error(JSON_ERROR_NONE);
        assert_eq!(
            services.json_last_error(),
            (JSON_ERROR_NONE, json_error_message(JSON_ERROR_NONE))
        );
    }

    #[test]
    fn pcre_last_error_state_updates_and_reads_from_extension_state() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);

        let mut services = context.pcre_services();
        services.set_preg_last_error(2, "backtrack limit exhausted");
        assert_eq!(services.preg_last_error(), (2, "backtrack limit exhausted"));

        services.clear_preg_last_error();
        assert_eq!(
            services.preg_last_error(),
            (
                pcre::PREG_NO_ERROR,
                pcre::preg_error_message(pcre::PREG_NO_ERROR)
            )
        );
    }

    #[test]
    fn pcre_last_error_has_one_request_owner_across_contexts() {
        let mut output = OutputBuffer::new();
        let mut request_state = BuiltinRequestState::new();

        {
            let mut context =
                BuiltinContext::new_with_request_state(&mut output, &mut request_state);
            let mut services = context.pcre_services();
            services.set_preg_last_error(3, "recursive limit exhausted");
            assert_eq!(services.preg_last_error(), (3, "recursive limit exhausted"));
        }

        assert_eq!(request_state.pcre().last_error().code(), 3);
        assert_eq!(
            request_state.pcre().last_error().message(),
            "recursive limit exhausted"
        );
    }

    #[test]
    fn session_sync_helpers_roundtrip_the_live_session_global() {
        let mut output = OutputBuffer::new();
        let mut state = SessionState::default();
        let global = ReferenceCell::new(Value::Array(PhpArray::new()));
        let mut seeded = PhpArray::new();
        seeded.insert(ArrayKey::String(PhpString::from("n")), Value::Int(7));
        state.set_data(seeded);

        {
            let mut context = BuiltinContext::new(&mut output);
            context.set_session_state(&mut state, global.clone());
            context.sync_session_global_from_state();
        }

        let Value::Array(array) = global.get() else {
            panic!("session global should contain an array");
        };
        assert_eq!(
            array.get(&ArrayKey::String(PhpString::from("n"))),
            Some(&Value::Int(7))
        );

        let mut edited = PhpArray::new();
        edited.insert(ArrayKey::String(PhpString::from("m")), Value::Int(11));
        global.set(Value::Array(edited));

        {
            let mut context = BuiltinContext::new(&mut output);
            context.set_session_state(&mut state, global);
            context.sync_session_state_from_global();
        }

        assert_eq!(
            state.data().get(&ArrayKey::String(PhpString::from("m"))),
            Some(&Value::Int(11))
        );
    }

    #[test]
    fn http_response_mutation_writes_to_live_response_state() {
        let mut output = OutputBuffer::new();
        let mut response = RuntimeHttpResponseState::default();

        {
            let mut context = BuiltinContext::new(&mut output);
            context.set_http_response_state(&mut response);
            assert!(context.http_response_mut().set_status_code(404));
            context
                .http_response_mut()
                .add_header_line("X-Test: yes", true, None)
                .expect("valid test header should be accepted");
        }

        assert_eq!(response.status_code, 404);
        assert_eq!(response.headers_list(), vec!["X-Test: yes"]);
    }

    #[test]
    fn warning_emission_writes_output_and_structured_diagnostic() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);

        context.php_warning(
            "E_PHP_RUNTIME_TEST_WARNING",
            "fixture warning",
            RuntimeSourceSpan {
                file: Some("fixture.php".to_owned()),
                start: 0,
                end: 7,
            },
        );

        let diagnostics = context.take_diagnostics();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].id(), "E_PHP_RUNTIME_TEST_WARNING");
        assert_eq!(diagnostics[0].message(), "fixture warning");
        drop(context);

        let rendered = std::str::from_utf8(output.as_bytes()).expect("warning output is utf-8");
        assert!(rendered.contains("Warning: fixture warning"));
        assert!(rendered.contains("fixture.php"));
    }

    #[test]
    fn upload_registry_is_exposed_through_http_service() {
        let mut output = OutputBuffer::new();
        let upload = RuntimeUploadedFile {
            field_name: "file".to_owned(),
            client_filename: "test.txt".to_owned(),
            content_type: "text/plain".to_owned(),
            temp_path: "/tmp/php-upload-test".to_owned(),
            error: 0,
            size: 4,
        };
        let mut registry = UploadRegistry::from_uploaded_files(&[upload]);

        {
            let mut context = BuiltinContext::new(&mut output);
            context.set_upload_registry(&mut registry);
            assert!(
                context
                    .upload_registry()
                    .is_some_and(|registry| registry.is_active_upload("/tmp/php-upload-test"))
            );
            assert!(
                context
                    .upload_registry_mut()
                    .is_some_and(|registry| registry.mark_moved("/tmp/php-upload-test"))
            );
        }

        assert!(!registry.is_active_upload("/tmp/php-upload-test"));
    }

    #[test]
    fn filesystem_cwd_and_include_path_are_isolated_per_context() {
        let mut first_output = OutputBuffer::new();
        let mut second_output = OutputBuffer::new();
        let mut first = BuiltinContext::new(&mut first_output);
        let mut second = BuiltinContext::new(&mut second_output);

        first.set_cwd("/tmp/first");
        first.set_include_path(vec![PathBuf::from("/tmp/first/include")]);
        second.set_cwd("/tmp/second");
        second.set_include_path(vec![PathBuf::from("/tmp/second/include")]);

        assert_eq!(first.cwd(), PathBuf::from("/tmp/first").as_path());
        assert_eq!(first.include_path(), &[PathBuf::from("/tmp/first/include")]);
        assert_eq!(second.cwd(), PathBuf::from("/tmp/second").as_path());
        assert_eq!(
            second.include_path(),
            &[PathBuf::from("/tmp/second/include")]
        );
    }

    #[test]
    fn strtok_consumes_terminating_delimiter_across_delimiter_sets() {
        // Regression: each strtok() call must advance past the delimiter that
        // terminated the previous token, so a later call with a different
        // delimiter set does not re-read it. Mirrors tests/strings/001.phpt.
        let mut state = StrtokState::default();
        state.reset(b"testing 1/2\\3".to_vec());
        assert_eq!(state.next_token(b" "), Some(b"testing".to_vec()));
        assert_eq!(state.next_token(b"/"), Some(b"1".to_vec()));
        assert_eq!(state.next_token(b"\\"), Some(b"2".to_vec()));
        assert_eq!(state.next_token(b"."), Some(b"3".to_vec()));
        assert_eq!(state.next_token(b" "), None);
    }

    #[test]
    fn strtok_skips_leading_and_repeated_delimiters() {
        let mut state = StrtokState::default();
        state.reset(b"a,,b".to_vec());
        assert_eq!(state.next_token(b","), Some(b"a".to_vec()));
        assert_eq!(state.next_token(b","), Some(b"b".to_vec()));
        assert_eq!(state.next_token(b","), None);
    }
}
