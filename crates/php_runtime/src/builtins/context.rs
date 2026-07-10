//! Runtime services passed to internal builtins.

use crate::{
    FilesystemCapabilities, IniRegistry, MysqlState, ObjectRef, OutputBuffer, PHP_E_DEPRECATED,
    PHP_E_NOTICE, PHP_E_WARNING, PcreCache, PhpArray, PhpDiagnosticChannel,
    PhpDiagnosticDisplayOptions, PostgresState, ReferenceCell, ResourceTable, RuntimeDiagnostic,
    RuntimeHttpResponseState, RuntimeSeverity, SessionLoadCallback, SessionState, UploadRegistry,
    Value, datetime, emit_php_diagnostic, pcre,
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

/// SysV message queue would-block errno used by the deterministic backend.
pub const SYSVMSG_EAGAIN: i64 = libc::EAGAIN as i64;
pub const SYSVMSG_EINVAL: i64 = libc::EINVAL as i64;
pub const SYSVMSG_IPC_NOWAIT: i64 = libc::IPC_NOWAIT as i64;

/// Request-local state for the CLI-only `pcntl` extension.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PcntlState {
    last_error: i32,
    async_signals: bool,
    signal_handlers: BTreeMap<i64, Value>,
    fork_child: bool,
    fork_observed: bool,
}

impl PcntlState {
    /// Last host errno observed by a PCNTL call.
    #[must_use]
    pub const fn last_error(&self) -> i32 {
        self.last_error
    }

    /// Updates the last host errno observed by a PCNTL call.
    pub fn set_last_error(&mut self, error: i32) {
        self.last_error = error;
    }

    /// Whether async signal dispatch is enabled for this request.
    #[must_use]
    pub const fn async_signals(&self) -> bool {
        self.async_signals
    }

    /// Updates async signal dispatch and returns the previous setting.
    pub fn set_async_signals(&mut self, enabled: bool) -> bool {
        let previous = self.async_signals;
        self.async_signals = enabled;
        previous
    }

    /// Stores a PHP-visible signal handler value.
    pub fn set_signal_handler(&mut self, signal: i64, handler: Value) {
        self.signal_handlers.insert(signal, handler);
    }

    /// Returns a PHP-visible signal handler value.
    #[must_use]
    pub fn signal_handler(&self, signal: i64) -> Option<Value> {
        self.signal_handlers.get(&signal).cloned()
    }

    /// Marks whether this request is executing in the child side of `pcntl_fork`.
    pub fn set_fork_child(&mut self, fork_child: bool) {
        self.fork_child = fork_child;
        self.fork_observed = true;
    }

    /// Returns whether this request is executing in the child side of `pcntl_fork`.
    #[must_use]
    pub const fn is_fork_child(&self) -> bool {
        self.fork_child
    }

    /// Returns whether this request has passed through `pcntl_fork`.
    #[must_use]
    pub const fn has_forked(&self) -> bool {
        self.fork_observed
    }
}

/// Request-local state for ext/curl handles and libcurl multi runtimes.
#[derive(Default)]
pub struct CurlState {
    handles: BTreeMap<u64, CurlHandleState>,
    pub(in crate::builtins) multis: BTreeMap<u64, CurlMultiRuntimeState>,
    pub(in crate::builtins) shares: BTreeMap<u64, CurlShareRuntimeState>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct CurlHandleState {
    closed: bool,
    options: BTreeMap<i64, Value>,
}

#[derive(Default)]
pub(in crate::builtins) struct CurlEasyCollector {
    pub headers: Vec<u8>,
    current_header_block: Vec<u8>,
    pub body: Vec<u8>,
}

impl Handler for CurlEasyCollector {
    fn write(&mut self, data: &[u8]) -> Result<usize, WriteError> {
        self.body.extend_from_slice(data);
        Ok(data.len())
    }

    fn header(&mut self, data: &[u8]) -> bool {
        if data.starts_with(b"HTTP/") {
            self.current_header_block.clear();
        }
        self.current_header_block.extend_from_slice(data);
        if data == b"\r\n" {
            self.headers.clone_from(&self.current_header_block);
        }
        true
    }
}

pub(in crate::builtins) struct CurlMultiRuntimeState {
    pub multi: Multi,
    pub transfers: BTreeMap<u64, CurlMultiTransferState>,
    pub pending: VecDeque<CurlMultiDone>,
    pub closed: bool,
}

impl Default for CurlMultiRuntimeState {
    fn default() -> Self {
        Self {
            multi: Multi::new(),
            transfers: BTreeMap::new(),
            pending: VecDeque::new(),
            closed: false,
        }
    }
}

pub(in crate::builtins) struct CurlMultiTransferState {
    pub object: ObjectRef,
    pub easy: Easy2Handle<CurlEasyCollector>,
    pub completed: bool,
}

#[derive(Clone)]
pub(in crate::builtins) struct CurlMultiDone {
    pub handle: ObjectRef,
    pub result: i64,
}

#[derive(Debug, Default)]
pub(in crate::builtins) struct CurlShareRuntimeState {
    pub shared_options: BTreeSet<i64>,
    pub closed: bool,
}

impl std::fmt::Debug for CurlState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CurlState")
            .field("handles", &self.handles)
            .field("multis", &self.multis.keys().collect::<Vec<_>>())
            .field("shares", &self.shares)
            .finish()
    }
}

impl CurlState {
    /// Ensures a handle has request-local cURL state.
    pub fn reset_handle(&mut self, handle_id: u64) {
        self.handles.insert(handle_id, CurlHandleState::default());
    }

    /// Copies request-local cURL state from one handle identity to another.
    pub fn copy_handle(&mut self, source_id: u64, target_id: u64) {
        let state = self.handles.get(&source_id).cloned().unwrap_or_default();
        self.handles.insert(target_id, state);
    }

    /// Marks a cURL handle as closed.
    pub fn close_handle(&mut self, handle_id: u64) {
        self.handles.entry(handle_id).or_default().closed = true;
    }

    /// Returns whether a cURL handle has been closed.
    #[must_use]
    pub fn is_closed(&self, handle_id: u64) -> bool {
        self.handles
            .get(&handle_id)
            .map(|state| state.closed)
            .unwrap_or(false)
    }

    /// Stores a PHP-visible cURL option in request-local typed state.
    pub fn set_option(&mut self, handle_id: u64, option: i64, value: Value) {
        self.handles
            .entry(handle_id)
            .or_default()
            .options
            .insert(option, value);
    }

    /// Returns a snapshot of request-local cURL options for execution.
    #[must_use]
    pub fn options_snapshot(&self, handle_id: u64) -> BTreeMap<i64, Value> {
        self.handles
            .get(&handle_id)
            .map(|state| state.options.clone())
            .unwrap_or_default()
    }

    /// Creates or resets a request-local libcurl multi runtime.
    pub fn reset_multi(&mut self, multi_id: u64) {
        self.multis
            .insert(multi_id, CurlMultiRuntimeState::default());
    }

    /// Returns mutable request-local libcurl multi runtime state.
    pub(in crate::builtins) fn multi_mut(
        &mut self,
        multi_id: u64,
    ) -> Option<&mut CurlMultiRuntimeState> {
        self.multis.get_mut(&multi_id)
    }

    /// Removes any active transfer for a handle from all multi runtimes.
    pub fn detach_handle_from_multis(&mut self, handle_id: u64) {
        for multi in self.multis.values_mut() {
            multi.transfers.remove(&handle_id);
            multi.pending.retain(|done| done.handle.id() != handle_id);
        }
    }

    /// Marks a multi runtime closed.
    pub fn close_multi(&mut self, multi_id: u64) {
        if let Some(multi) = self.multis.get_mut(&multi_id) {
            multi.closed = true;
            multi.transfers.clear();
            multi.pending.clear();
        }
    }

    /// Creates or resets a request-local cURL share runtime.
    pub fn reset_share(&mut self, share_id: u64) {
        self.shares
            .insert(share_id, CurlShareRuntimeState::default());
    }

    /// Returns mutable request-local share state.
    pub(in crate::builtins) fn share_mut(
        &mut self,
        share_id: u64,
    ) -> Option<&mut CurlShareRuntimeState> {
        self.shares.get_mut(&share_id)
    }
}

/// Request-local state for loopback-gated FTP connections.
#[derive(Debug, Default)]
pub struct FtpState {
    next_id: i64,
    connections: BTreeMap<i64, FtpEntry>,
}

/// Request-local state for the LDAP facade.
#[derive(Debug, Default)]
pub struct LdapState {
    next_connection_id: i64,
    next_result_id: i64,
    next_entry_id: i64,
    connections: BTreeMap<i64, LdapConnectionState>,
    backends: BTreeMap<i64, LdapConn>,
    results: BTreeMap<i64, LdapResultState>,
    entries: BTreeMap<i64, LdapResultEntryState>,
    global_options: BTreeMap<i64, Value>,
}

/// Request-local state for the IMAP facade.
#[derive(Debug, Default)]
pub struct ImapState {
    next_connection_id: i64,
    connections: BTreeMap<i64, ImapConnectionState>,
    backends: BTreeMap<i64, ImapBackendState>,
    last_errors: Vec<String>,
    last_alerts: Vec<String>,
}

/// Request-local state for the SSH2 facade and opt-in libssh2 backend.
#[derive(Default)]
pub struct Ssh2State {
    next_session_id: i64,
    next_sftp_id: i64,
    sessions: BTreeMap<i64, Ssh2SessionState>,
    sftp_handles: BTreeMap<i64, Ssh2SftpState>,
    backends: BTreeMap<i64, Ssh2BackendState>,
    sftp_backends: BTreeMap<i64, Ssh2BackendSftp>,
}

impl std::fmt::Debug for Ssh2State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ssh2State")
            .field("next_session_id", &self.next_session_id)
            .field("next_sftp_id", &self.next_sftp_id)
            .field("sessions", &self.sessions)
            .field("sftp_handles", &self.sftp_handles)
            .field("backends", &self.backends.keys().collect::<Vec<_>>())
            .field(
                "sftp_backends",
                &self.sftp_backends.keys().collect::<Vec<_>>(),
            )
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Ssh2SessionState {
    host: String,
    port: i64,
    authenticated: bool,
    last_error: String,
    closed: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct Ssh2SftpState {
    session_id: i64,
    closed: bool,
}

struct Ssh2BackendState {
    session: Ssh2BackendSession,
}

impl std::fmt::Debug for Ssh2BackendState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ssh2BackendState").finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ImapConnectionState {
    mailbox: String,
    flags: i64,
    closed: bool,
    deleted_messages: BTreeSet<i64>,
}

struct ImapBackendState {
    session: imap::Session<imap::Connection>,
    mailbox: imap::types::Mailbox,
}

impl std::fmt::Debug for ImapBackendState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImapBackendState")
            .field("mailbox", &self.mailbox)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, PartialEq)]
struct LdapConnectionState {
    uri: Option<String>,
    port: i64,
    options: BTreeMap<i64, Value>,
    last_errno: i64,
    last_error: String,
    closed: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct LdapResultState {
    entries: Vec<i64>,
    freed: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct LdapResultEntryState {
    dn: String,
    attributes: PhpArray,
    next_entry_id: Option<i64>,
}

impl Ssh2State {
    /// Creates a request-local SSH2 session handle without opening sockets.
    pub fn connect(&mut self, host: String, port: i64) -> i64 {
        let id = if self.next_session_id <= 0 {
            1
        } else {
            self.next_session_id
        };
        self.next_session_id = id.saturating_add(1);
        self.sessions.insert(
            id,
            Ssh2SessionState {
                host,
                port,
                authenticated: false,
                last_error: "SSH2 backend is not configured".to_owned(),
                closed: false,
            },
        );
        id
    }

    /// Opens a libssh2 backend for an existing request-local session handle.
    pub fn connect_backend(&mut self, id: i64) -> bool {
        let Some(session_state) = self.sessions.get(&id) else {
            return false;
        };
        if session_state.closed {
            return false;
        }
        let port = match u16::try_from(session_state.port) {
            Ok(port) => port,
            Err(_) => {
                self.set_error(id, "invalid SSH2 port");
                return false;
            }
        };
        let address = (session_state.host.as_str(), port);
        let tcp = match TcpStream::connect(address) {
            Ok(tcp) => tcp,
            Err(error) => {
                self.set_error(id, error.to_string());
                return false;
            }
        };
        let mut backend = match Ssh2BackendSession::new() {
            Ok(backend) => backend,
            Err(error) => {
                self.set_error(id, error.to_string());
                return false;
            }
        };
        backend.set_tcp_stream(tcp);
        if let Err(error) = backend.handshake() {
            self.set_error(id, error.to_string());
            return false;
        }
        self.backends
            .insert(id, Ssh2BackendState { session: backend });
        self.set_error(id, "");
        true
    }

    /// Returns whether a live backend is attached to the request-local session.
    #[must_use]
    pub fn has_backend(&self, id: i64) -> bool {
        self.backends.contains_key(&id)
    }

    /// Returns whether a request-local SSH2 session is open.
    #[must_use]
    pub fn is_open(&self, id: i64) -> bool {
        self.sessions
            .get(&id)
            .is_some_and(|session| !session.closed)
    }

    /// Closes a request-local SSH2 session.
    pub fn close(&mut self, id: i64) -> bool {
        let Some(session) = self.sessions.get_mut(&id) else {
            return false;
        };
        session.closed = true;
        if let Some(backend) = self.backends.remove(&id) {
            let _ = backend
                .session
                .disconnect(None, "ssh2_disconnect called", None);
        }
        self.sftp_backends.retain(|sftp_id, _| {
            self.sftp_handles
                .get(sftp_id)
                .is_some_and(|sftp| sftp.session_id != id)
        });
        for sftp in self
            .sftp_handles
            .values_mut()
            .filter(|sftp| sftp.session_id == id)
        {
            sftp.closed = true;
        }
        true
    }

    /// Records an SSH2 backend/authentication error.
    pub fn set_error(&mut self, id: i64, error: impl Into<String>) {
        if let Some(session) = self.sessions.get_mut(&id) {
            session.last_error = error.into();
        }
    }

    /// Returns the deterministic session error.
    #[must_use]
    pub fn error(&self, id: i64) -> Option<String> {
        self.sessions.get(&id).map(|session| {
            if session.last_error.is_empty() {
                return String::new();
            }
            format!(
                "{} for {}:{}",
                session.last_error, session.host, session.port
            )
        })
    }

    /// Authenticates a live SSH2 session with a password.
    pub fn auth_password_backend(
        &mut self,
        id: i64,
        username: &str,
        password: &str,
    ) -> Option<bool> {
        let backend = self.backends.get_mut(&id)?;
        match backend.session.userauth_password(username, password) {
            Ok(()) => {
                if let Some(session) = self.sessions.get_mut(&id) {
                    session.authenticated = backend.session.authenticated();
                    session.last_error.clear();
                }
                Some(true)
            }
            Err(error) => {
                self.set_error(id, error.to_string());
                Some(false)
            }
        }
    }

    /// Authenticates a live SSH2 session with public/private key files.
    pub fn auth_pubkey_file_backend(
        &mut self,
        id: i64,
        username: &str,
        pubkey: &Path,
        privatekey: &Path,
        passphrase: Option<&str>,
    ) -> Option<bool> {
        let backend = self.backends.get_mut(&id)?;
        match backend
            .session
            .userauth_pubkey_file(username, Some(pubkey), privatekey, passphrase)
        {
            Ok(()) => {
                if let Some(session) = self.sessions.get_mut(&id) {
                    session.authenticated = backend.session.authenticated();
                    session.last_error.clear();
                }
                Some(true)
            }
            Err(error) => {
                self.set_error(id, error.to_string());
                Some(false)
            }
        }
    }

    /// Executes a command through a live SSH2 session and returns stdout bytes.
    pub fn exec_backend(&mut self, id: i64, command: &str) -> Option<Vec<u8>> {
        let backend = self.backends.get_mut(&id)?;
        let mut channel = match backend.session.channel_session() {
            Ok(channel) => channel,
            Err(error) => {
                self.set_error(id, error.to_string());
                return None;
            }
        };
        if let Err(error) = channel.exec(command) {
            self.set_error(id, error.to_string());
            return None;
        }
        let mut output = Vec::new();
        if let Err(error) = channel.read_to_end(&mut output) {
            self.set_error(id, error.to_string());
            return None;
        }
        if let Err(error) = channel.wait_close() {
            self.set_error(id, error.to_string());
            return None;
        }
        self.set_error(id, "");
        Some(output)
    }

    /// Creates a request-local SFTP handle attached to a session.
    pub fn sftp(&mut self, session_id: i64) -> Option<i64> {
        if !self.is_open(session_id) {
            return None;
        }
        let backend_sftp = if let Some(backend) = self.backends.get(&session_id) {
            match backend.session.sftp() {
                Ok(sftp) => Some(sftp),
                Err(error) => {
                    self.set_error(session_id, error.to_string());
                    return None;
                }
            }
        } else {
            None
        };
        let id = if self.next_sftp_id <= 0 {
            1
        } else {
            self.next_sftp_id
        };
        self.next_sftp_id = id.saturating_add(1);
        self.sftp_handles.insert(
            id,
            Ssh2SftpState {
                session_id,
                closed: false,
            },
        );
        if let Some(sftp) = backend_sftp {
            self.sftp_backends.insert(id, sftp);
        }
        Some(id)
    }

    /// Returns whether an SFTP handle is attached to an open session.
    #[must_use]
    pub fn sftp_is_open(&self, id: i64) -> bool {
        self.sftp_handles
            .get(&id)
            .filter(|sftp| !sftp.closed)
            .is_some_and(|sftp| self.is_open(sftp.session_id))
    }

    /// Copies a remote SCP file to a local path through the live SSH2 backend.
    pub fn scp_recv_backend(&mut self, id: i64, remote: &Path, local: &Path) -> Option<bool> {
        let backend = self.backends.get_mut(&id)?;
        let (mut remote_file, _) = match backend.session.scp_recv(remote) {
            Ok(file) => file,
            Err(error) => {
                self.set_error(id, error.to_string());
                return Some(false);
            }
        };
        let mut bytes = Vec::new();
        if let Err(error) = remote_file.read_to_end(&mut bytes) {
            self.set_error(id, error.to_string());
            return Some(false);
        }
        if let Err(error) = std::fs::write(local, bytes) {
            self.set_error(id, error.to_string());
            return Some(false);
        }
        self.set_error(id, "");
        Some(true)
    }

    /// Copies a local file to a remote SCP path through the live SSH2 backend.
    pub fn scp_send_backend(
        &mut self,
        id: i64,
        local: &Path,
        remote: &Path,
        mode: i32,
    ) -> Option<bool> {
        let bytes = match std::fs::read(local) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.set_error(id, error.to_string());
                return if self.backends.contains_key(&id) {
                    Some(false)
                } else {
                    None
                };
            }
        };
        let backend = self.backends.get_mut(&id)?;
        let mut remote_file = match backend
            .session
            .scp_send(remote, mode, bytes.len() as u64, None)
        {
            Ok(file) => file,
            Err(error) => {
                self.set_error(id, error.to_string());
                return Some(false);
            }
        };
        if let Err(error) = remote_file.write_all(&bytes) {
            self.set_error(id, error.to_string());
            return Some(false);
        }
        if let Err(error) = remote_file.send_eof() {
            self.set_error(id, error.to_string());
            return Some(false);
        }
        if let Err(error) = remote_file.wait_eof() {
            self.set_error(id, error.to_string());
            return Some(false);
        }
        if let Err(error) = remote_file.close() {
            self.set_error(id, error.to_string());
            return Some(false);
        }
        self.set_error(id, "");
        Some(true)
    }

    /// Returns the live host-key fingerprint bytes for a session.
    pub fn fingerprint_backend(&mut self, id: i64, hash: Ssh2FingerprintHash) -> Option<Vec<u8>> {
        let backend = self.backends.get(&id)?;
        let hash_type = match hash {
            Ssh2FingerprintHash::Md5 => HashType::Md5,
            Ssh2FingerprintHash::Sha1 => HashType::Sha1,
        };
        backend.session.host_key_hash(hash_type).map(<[u8]>::to_vec)
    }

    /// Returns whether a session is marked authenticated.
    #[must_use]
    pub fn is_authenticated(&self, id: i64) -> bool {
        self.sessions
            .get(&id)
            .is_some_and(|session| session.authenticated && !session.closed)
    }
}

/// Hash algorithm selected by SSH2 fingerprint flags.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Ssh2FingerprintHash {
    Md5,
    Sha1,
}

impl ImapState {
    /// Opens a request-local IMAP handle without connecting to a mail server.
    pub fn open(&mut self, mailbox: String, flags: i64) -> i64 {
        let id = if self.next_connection_id <= 0 {
            1
        } else {
            self.next_connection_id
        };
        self.next_connection_id = id.saturating_add(1);
        self.connections.insert(
            id,
            ImapConnectionState {
                mailbox,
                flags,
                closed: false,
                deleted_messages: BTreeSet::new(),
            },
        );
        id
    }

    /// Closes a request-local IMAP handle.
    pub fn close(&mut self, id: i64) -> bool {
        let Some(connection) = self.connections.get_mut(&id) else {
            return false;
        };
        connection.closed = true;
        if let Some(mut backend) = self.backends.remove(&id) {
            let _ = backend.session.logout();
        }
        true
    }

    /// Returns whether a request-local IMAP handle is open.
    #[must_use]
    pub fn is_open(&self, id: i64) -> bool {
        self.connections
            .get(&id)
            .is_some_and(|connection| !connection.closed)
    }

    /// Returns a connection mailbox name when the handle is open.
    #[must_use]
    pub fn mailbox(&self, id: i64) -> Option<String> {
        self.connections
            .get(&id)
            .filter(|connection| !connection.closed)
            .map(|connection| connection.mailbox.clone())
    }

    /// Returns open flags for the handle.
    #[must_use]
    pub fn flags(&self, id: i64) -> Option<i64> {
        self.connections
            .get(&id)
            .filter(|connection| !connection.closed)
            .map(|connection| connection.flags)
    }

    /// Opens a live IMAP backend session for an existing request-local handle.
    pub fn open_backend(
        &mut self,
        id: i64,
        config: &ImapConnectionConfig,
        user: &str,
        password: &str,
    ) -> bool {
        if !self.is_open(id) {
            return false;
        }
        let mode = if config.ssl {
            ConnectionMode::Tls
        } else {
            ConnectionMode::Plaintext
        };
        let builder = ClientBuilder::new(config.host.as_str(), config.port)
            .mode(mode)
            .danger_skip_tls_verify(config.novalidate_cert);
        let client = match builder.connect() {
            Ok(client) => client,
            Err(error) => {
                self.push_error(error.to_string());
                return false;
            }
        };
        let mut session = match client.login(user, password) {
            Ok(session) => session,
            Err((error, _client)) => {
                self.push_error(error.to_string());
                return false;
            }
        };
        let mailbox = match session.select(&config.mailbox) {
            Ok(mailbox) => mailbox,
            Err(error) => {
                self.push_error(error.to_string());
                let _ = session.logout();
                return false;
            }
        };
        self.backends
            .insert(id, ImapBackendState { session, mailbox });
        true
    }

    /// Returns whether a live backend is attached to a request-local handle.
    #[must_use]
    pub fn has_backend(&self, id: i64) -> bool {
        self.backends.contains_key(&id)
    }

    /// Returns live mailbox metadata for check/status/info functions.
    pub fn backend_mailbox(&mut self, id: i64) -> Option<ImapMailboxSnapshot> {
        let backend = self.backends.get_mut(&id)?;
        if let Err(error) = backend.session.noop() {
            self.push_error(error.to_string());
            return None;
        }
        Some(ImapMailboxSnapshot::from(&backend.mailbox))
    }

    /// Fetches live message header bytes from the backend.
    pub fn backend_fetch_header(&mut self, id: i64, message: i64) -> Option<Vec<u8>> {
        let backend = self.backends.get_mut(&id)?;
        let fetches = match backend
            .session
            .fetch(message.to_string(), "BODY.PEEK[HEADER]")
        {
            Ok(fetches) => fetches,
            Err(error) => {
                self.push_error(error.to_string());
                return None;
            }
        };
        fetches
            .iter()
            .next()
            .and_then(|message| message.header().map(<[u8]>::to_vec))
    }

    /// Fetches live message body bytes from the backend.
    pub fn backend_fetch_body(&mut self, id: i64, message: i64) -> Option<Vec<u8>> {
        let backend = self.backends.get_mut(&id)?;
        let fetches = match backend.session.fetch(message.to_string(), "BODY.PEEK[]") {
            Ok(fetches) => fetches,
            Err(error) => {
                self.push_error(error.to_string());
                return None;
            }
        };
        fetches
            .iter()
            .next()
            .and_then(|message| message.body().map(<[u8]>::to_vec))
    }

    /// Fetches live header summaries from the backend.
    pub fn backend_headers(&mut self, id: i64) -> Option<Vec<String>> {
        let count = self.backends.get(&id)?.mailbox.exists;
        if count == 0 {
            return Some(Vec::new());
        }
        let backend = self.backends.get_mut(&id)?;
        let fetches = match backend.session.fetch("1:*", "BODY.PEEK[HEADER]") {
            Ok(fetches) => fetches,
            Err(error) => {
                self.push_error(error.to_string());
                return None;
            }
        };
        Some(
            fetches
                .iter()
                .filter_map(|message| {
                    message
                        .header()
                        .map(|header| String::from_utf8_lossy(header).into_owned())
                })
                .collect(),
        )
    }

    /// Searches the live backend and returns matching message numbers.
    pub fn backend_search(&mut self, id: i64, criteria: &str) -> Option<Vec<i64>> {
        let backend = self.backends.get_mut(&id)?;
        let matches = match backend.session.search(criteria) {
            Ok(matches) => matches,
            Err(error) => {
                self.push_error(error.to_string());
                return None;
            }
        };
        let mut messages = matches.into_iter().map(i64::from).collect::<Vec<_>>();
        messages.sort_unstable();
        Some(messages)
    }

    /// Marks a message as deleted in request-local state.
    pub fn mark_deleted(&mut self, id: i64, message: i64) -> bool {
        let Some(connection) = self.connections.get_mut(&id) else {
            return false;
        };
        if connection.closed || message <= 0 {
            return false;
        }
        connection.deleted_messages.insert(message);
        true
    }

    /// Removes all deletion markers for deterministic empty mailboxes.
    pub fn expunge(&mut self, id: i64) -> bool {
        let Some(connection) = self.connections.get_mut(&id) else {
            return false;
        };
        if connection.closed {
            return false;
        }
        connection.deleted_messages.clear();
        true
    }

    /// Records an IMAP error string.
    pub fn push_error(&mut self, error: impl Into<String>) {
        self.last_errors.push(error.into());
    }

    /// Returns and clears IMAP error strings.
    #[must_use]
    pub fn take_errors(&mut self) -> Vec<String> {
        std::mem::take(&mut self.last_errors)
    }

    /// Returns the most recent IMAP error string.
    #[must_use]
    pub fn last_error(&self) -> Option<String> {
        self.last_errors.last().cloned()
    }

    /// Returns and clears IMAP alert strings.
    #[must_use]
    pub fn take_alerts(&mut self) -> Vec<String> {
        std::mem::take(&mut self.last_alerts)
    }
}

/// Parsed PHP IMAP mailbox connection string.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImapConnectionConfig {
    pub host: String,
    pub port: u16,
    pub ssl: bool,
    pub novalidate_cert: bool,
    pub mailbox: String,
}

/// Stable mailbox metadata exposed to IMAP builtins.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImapMailboxSnapshot {
    pub exists: i64,
    pub recent: i64,
    pub unseen: i64,
    pub uid_next: i64,
    pub uid_validity: i64,
}

impl From<&imap::types::Mailbox> for ImapMailboxSnapshot {
    fn from(mailbox: &imap::types::Mailbox) -> Self {
        Self {
            exists: i64::from(mailbox.exists),
            recent: i64::from(mailbox.recent),
            unseen: mailbox.unseen.map_or(0, i64::from),
            uid_next: mailbox.uid_next.map_or(0, i64::from),
            uid_validity: mailbox.uid_validity.map_or(0, i64::from),
        }
    }
}

impl LdapState {
    /// Creates a request-local LDAP connection handle without opening sockets.
    pub fn connect(&mut self, uri: Option<String>, port: i64) -> i64 {
        let id = if self.next_connection_id <= 0 {
            1
        } else {
            self.next_connection_id
        };
        self.next_connection_id = id.saturating_add(1);
        let mut options = BTreeMap::new();
        options.insert(OPT_PROTOCOL_VERSION, Value::Int(2));
        options.insert(OPT_REFERRALS, Value::Bool(true));
        options.insert(OPT_DEREF, Value::Int(0));
        options.insert(OPT_SIZELIMIT, Value::Int(-1));
        options.insert(OPT_TIMELIMIT, Value::Int(-1));
        options.insert(OPT_NETWORK_TIMEOUT, Value::Int(-1));
        options.insert(OPT_X_TLS_REQUIRE_CERT, Value::Int(2));
        self.connections.insert(
            id,
            LdapConnectionState {
                uri,
                port,
                options,
                last_errno: 0,
                last_error: ldap_error_message(0).to_owned(),
                closed: false,
            },
        );
        id
    }

    /// Closes a request-local LDAP handle.
    pub fn close(&mut self, id: i64) -> bool {
        let Some(connection) = self.connections.get_mut(&id) else {
            return false;
        };
        connection.closed = true;
        self.backends.remove(&id);
        true
    }

    /// Returns whether a request-local LDAP handle is still open.
    #[must_use]
    pub fn is_open(&self, id: i64) -> bool {
        self.connections
            .get(&id)
            .is_some_and(|connection| !connection.closed)
    }

    /// Records a deterministic LDAP error on a connection handle.
    pub fn set_connection_error(&mut self, id: i64, errno: i64, error: impl Into<String>) {
        if let Some(connection) = self.connections.get_mut(&id) {
            connection.last_errno = errno;
            connection.last_error = error.into();
        }
    }

    /// Returns the configured LDAP URI for a request-local connection handle.
    #[must_use]
    pub fn connection_uri(&self, id: i64) -> Option<String> {
        let connection = self.connections.get(&id)?;
        (!connection.closed)
            .then(|| normalized_ldap_uri(connection.uri.as_deref(), connection.port))
    }

    /// Binds a live LDAP backend for a request-local connection handle.
    pub fn bind_backend(&mut self, id: i64, url: &str, bind_dn: &str, password: &str) -> bool {
        let Some(connection) = self.connections.get(&id) else {
            return false;
        };
        if connection.closed {
            return false;
        }
        if let std::collections::btree_map::Entry::Vacant(entry) = self.backends.entry(id) {
            match LdapConn::new(url) {
                Ok(backend) => {
                    entry.insert(backend);
                }
                Err(error) => {
                    let (errno, message) = ldap_backend_error(error);
                    if let Some(connection) = self.connections.get_mut(&id) {
                        connection.last_errno = errno;
                        connection.last_error = message;
                    }
                    return false;
                }
            }
        }
        let Some(backend) = self.backends.get_mut(&id) else {
            self.set_connection_error(id, -1, "LDAP backend unavailable");
            return false;
        };
        let bind = backend
            .simple_bind(bind_dn, password)
            .and_then(|result| result.success());
        match bind {
            Ok(_) => {
                self.set_connection_error(id, 0, ldap_error_message(0));
                true
            }
            Err(error) => {
                self.set_ldap_backend_error(id, error);
                false
            }
        }
    }

    /// Runs a live LDAP search and stores its result/entry handles.
    pub fn search_backend(
        &mut self,
        id: i64,
        url: &str,
        base: &str,
        scope: LdapSearchScope,
        filter: &str,
        attributes: Vec<String>,
    ) -> Option<i64> {
        if !self.is_open(id) {
            return None;
        }
        if let std::collections::btree_map::Entry::Vacant(entry) = self.backends.entry(id) {
            match LdapConn::new(url) {
                Ok(backend) => {
                    entry.insert(backend);
                }
                Err(error) => {
                    let (errno, message) = ldap_backend_error(error);
                    if let Some(connection) = self.connections.get_mut(&id) {
                        connection.last_errno = errno;
                        connection.last_error = message;
                    }
                    return None;
                }
            }
        }
        let scope = match scope {
            LdapSearchScope::Base => Scope::Base,
            LdapSearchScope::OneLevel => Scope::OneLevel,
            LdapSearchScope::Subtree => Scope::Subtree,
        };
        let Some(backend) = self.backends.get_mut(&id) else {
            self.set_connection_error(id, -1, "LDAP backend unavailable");
            return None;
        };
        let search = backend
            .search(base, scope, filter, attributes)
            .and_then(|result| result.success());
        match search {
            Ok((entries, _)) => {
                self.set_connection_error(id, 0, ldap_error_message(0));
                Some(self.store_search_entries(
                    entries.into_iter().map(SearchEntry::construct).collect(),
                ))
            }
            Err(error) => {
                self.set_ldap_backend_error(id, error);
                None
            }
        }
    }

    /// Returns the request-local LDAP errno for a connection handle.
    #[must_use]
    pub fn errno(&self, id: i64) -> i64 {
        self.connections
            .get(&id)
            .map_or(-1, |connection| connection.last_errno)
    }

    /// Returns the request-local LDAP error string for a connection handle.
    #[must_use]
    pub fn error(&self, id: i64) -> String {
        self.connections.get(&id).map_or_else(
            || ldap_error_message(-1).to_owned(),
            |connection| connection.last_error.clone(),
        )
    }

    /// Stores an LDAP option globally or on a connection handle.
    pub fn set_option(&mut self, id: Option<i64>, option: i64, value: Value) -> bool {
        if !is_supported_ldap_option(option) {
            return false;
        }
        if let Some(id) = id {
            let Some(connection) = self.connections.get_mut(&id) else {
                return false;
            };
            if connection.closed {
                return false;
            }
            connection.options.insert(option, value);
            return true;
        }
        self.global_options.insert(option, value);
        true
    }

    /// Reads an LDAP option globally or from a connection handle.
    #[must_use]
    pub fn option(&self, id: Option<i64>, option: i64) -> Option<Value> {
        if !is_supported_ldap_option(option) {
            return None;
        }
        if let Some(id) = id {
            let connection = self.connections.get(&id)?;
            if connection.closed {
                return None;
            }
            if let Some(value) = connection.options.get(&option) {
                return Some(value.clone());
            }
        }
        self.global_options
            .get(&option)
            .cloned()
            .or_else(|| ldap_default_option(option))
    }

    /// Creates an empty LDAP result object for deterministic local traversal tests.
    pub fn empty_result(&mut self) -> i64 {
        let id = if self.next_result_id <= 0 {
            1
        } else {
            self.next_result_id
        };
        self.next_result_id = id.saturating_add(1);
        self.results.insert(
            id,
            LdapResultState {
                entries: Vec::new(),
                freed: false,
            },
        );
        id
    }

    /// Frees a request-local LDAP result handle.
    pub fn free_result(&mut self, id: i64) -> bool {
        let Some(result) = self.results.get_mut(&id) else {
            return false;
        };
        result.freed = true;
        true
    }

    /// Counts entries in a request-local LDAP result.
    #[must_use]
    pub fn count_entries(&self, id: i64) -> Option<usize> {
        let result = self.results.get(&id)?;
        (!result.freed).then_some(result.entries.len())
    }

    /// Returns the first entry handle in a request-local LDAP result.
    #[must_use]
    pub fn first_entry(&self, id: i64) -> Option<i64> {
        let result = self.results.get(&id)?;
        if result.freed {
            return None;
        }
        result.entries.first().copied()
    }

    /// Returns the next entry handle for request-local LDAP traversal.
    #[must_use]
    pub fn next_entry(&self, id: i64) -> Option<i64> {
        self.entries.get(&id).and_then(|entry| entry.next_entry_id)
    }

    /// Returns an entry's distinguished name.
    #[must_use]
    pub fn entry_dn(&self, id: i64) -> Option<String> {
        self.entries.get(&id).map(|entry| entry.dn.clone())
    }

    /// Returns an entry's attribute array.
    #[must_use]
    pub fn entry_attributes(&self, id: i64) -> Option<PhpArray> {
        self.entries.get(&id).map(|entry| entry.attributes.clone())
    }

    /// Returns all entries in PHP ldap_get_entries shape.
    #[must_use]
    pub fn entries_array(&self, id: i64) -> Option<PhpArray> {
        let result = self.results.get(&id)?;
        if result.freed {
            return None;
        }
        let mut output = PhpArray::new();
        output.insert(
            crate::ArrayKey::String(crate::PhpString::from("count")),
            Value::Int(result.entries.len() as i64),
        );
        for (index, entry_id) in result.entries.iter().copied().enumerate() {
            let Some(entry) = self.entries.get(&entry_id) else {
                continue;
            };
            let mut entry_array = entry.attributes.clone();
            entry_array.insert(
                crate::ArrayKey::String(crate::PhpString::from("dn")),
                Value::string(entry.dn.clone()),
            );
            output.insert(
                crate::ArrayKey::Int(index as i64),
                Value::Array(entry_array),
            );
        }
        Some(output)
    }

    fn store_search_entries(&mut self, entries: Vec<SearchEntry>) -> i64 {
        let result_id = if self.next_result_id <= 0 {
            1
        } else {
            self.next_result_id
        };
        self.next_result_id = result_id.saturating_add(1);
        let mut entry_ids = Vec::with_capacity(entries.len());
        let mut previous_entry_id = None;
        for entry in entries {
            let entry_id = if self.next_entry_id <= 0 {
                1
            } else {
                self.next_entry_id
            };
            self.next_entry_id = entry_id.saturating_add(1);
            if let Some(previous_entry_id) = previous_entry_id
                && let Some(previous) = self.entries.get_mut(&previous_entry_id)
            {
                previous.next_entry_id = Some(entry_id);
            }
            previous_entry_id = Some(entry_id);
            entry_ids.push(entry_id);
            self.entries.insert(
                entry_id,
                LdapResultEntryState {
                    dn: entry.dn,
                    attributes: ldap_attributes_array(entry.attrs, entry.bin_attrs),
                    next_entry_id: None,
                },
            );
        }
        self.results.insert(
            result_id,
            LdapResultState {
                entries: entry_ids,
                freed: false,
            },
        );
        result_id
    }

    fn set_ldap_backend_error(&mut self, id: i64, error: LdapError) {
        let (errno, message) = ldap_backend_error(error);
        self.set_connection_error(id, errno, message);
    }
}

pub(crate) const OPT_DEREF: i64 = 2;
pub(crate) const OPT_SIZELIMIT: i64 = 3;
pub(crate) const OPT_TIMELIMIT: i64 = 4;
pub(crate) const OPT_NETWORK_TIMEOUT: i64 = 20485;
pub(crate) const OPT_TIMEOUT: i64 = 20482;
pub(crate) const OPT_PROTOCOL_VERSION: i64 = 17;
pub(crate) const OPT_ERROR_NUMBER: i64 = 49;
pub(crate) const OPT_REFERRALS: i64 = 8;
pub(crate) const OPT_RESTART: i64 = 9;
pub(crate) const OPT_HOST_NAME: i64 = 48;
pub(crate) const OPT_ERROR_STRING: i64 = 50;
pub(crate) const OPT_MATCHED_DN: i64 = 51;
pub(crate) const OPT_SERVER_CONTROLS: i64 = 18;
pub(crate) const OPT_CLIENT_CONTROLS: i64 = 19;
pub(crate) const OPT_DEBUG_LEVEL: i64 = 20481;
pub(crate) const OPT_X_TLS_REQUIRE_CERT: i64 = 24582;

/// LDAP search scope used by the runtime LDAP facade.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LdapSearchScope {
    Base,
    OneLevel,
    Subtree,
}

fn is_supported_ldap_option(option: i64) -> bool {
    matches!(
        option,
        OPT_DEREF
            | OPT_SIZELIMIT
            | OPT_TIMELIMIT
            | OPT_NETWORK_TIMEOUT
            | OPT_TIMEOUT
            | OPT_PROTOCOL_VERSION
            | OPT_ERROR_NUMBER
            | OPT_REFERRALS
            | OPT_RESTART
            | OPT_HOST_NAME
            | OPT_ERROR_STRING
            | OPT_MATCHED_DN
            | OPT_SERVER_CONTROLS
            | OPT_CLIENT_CONTROLS
            | OPT_DEBUG_LEVEL
            | OPT_X_TLS_REQUIRE_CERT
    )
}

fn ldap_default_option(option: i64) -> Option<Value> {
    match option {
        OPT_DEREF => Some(Value::Int(0)),
        OPT_SIZELIMIT | OPT_TIMELIMIT | OPT_NETWORK_TIMEOUT | OPT_TIMEOUT => Some(Value::Int(-1)),
        OPT_PROTOCOL_VERSION => Some(Value::Int(2)),
        OPT_ERROR_NUMBER => Some(Value::Int(0)),
        OPT_REFERRALS | OPT_RESTART => Some(Value::Bool(true)),
        OPT_HOST_NAME | OPT_ERROR_STRING | OPT_MATCHED_DN => Some(Value::string("")),
        OPT_SERVER_CONTROLS | OPT_CLIENT_CONTROLS => Some(Value::Array(PhpArray::new())),
        OPT_DEBUG_LEVEL => Some(Value::Int(0)),
        OPT_X_TLS_REQUIRE_CERT => Some(Value::Int(2)),
        _ => None,
    }
}

fn ldap_error_message(errno: i64) -> &'static str {
    match errno {
        0 => "Success",
        1 => "Operations error",
        2 => "Protocol error",
        32 => "No such object",
        34 => "Invalid DN syntax",
        49 => "Invalid credentials",
        80 => "Other (e.g., implementation specific) error",
        81 => "Can't contact LDAP server",
        -1 => "Can't contact LDAP server",
        _ => "Unknown error",
    }
}

fn normalized_ldap_uri(uri: Option<&str>, port: i64) -> String {
    let raw = uri
        .map(str::trim)
        .filter(|uri| !uri.is_empty())
        .unwrap_or("ldap://localhost");
    if raw.starts_with("ldapi://") {
        return raw.to_owned();
    }
    let with_scheme = if raw.contains("://") {
        raw.to_owned()
    } else {
        format!("ldap://{raw}")
    };
    let Some((scheme, rest)) = with_scheme.split_once("://") else {
        return with_scheme;
    };
    let split = rest.find('/').unwrap_or(rest.len());
    let authority = &rest[..split];
    let suffix = &rest[split..];
    let has_port = authority
        .rsplit_once(':')
        .is_some_and(|(_, candidate)| candidate.parse::<u16>().is_ok());
    if has_port {
        with_scheme
    } else {
        format!("{scheme}://{authority}:{port}{suffix}")
    }
}

fn ldap_backend_error(error: LdapError) -> (i64, String) {
    match error {
        LdapError::LdapResult { result } => {
            let errno = i64::from(result.rc);
            let message = if result.text.is_empty() {
                ldap_error_message(errno).to_owned()
            } else {
                result.text
            };
            (errno, message)
        }
        error => (81, error.to_string()),
    }
}

fn ldap_attributes_array(
    attrs: HashMap<String, Vec<String>>,
    bin_attrs: HashMap<String, Vec<Vec<u8>>>,
) -> PhpArray {
    let mut names = attrs
        .keys()
        .chain(bin_attrs.keys())
        .cloned()
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();

    let mut output = PhpArray::new();
    output.insert(
        crate::ArrayKey::String(crate::PhpString::from("count")),
        Value::Int(names.len() as i64),
    );
    for (index, name) in names.into_iter().enumerate() {
        output.insert(
            crate::ArrayKey::Int(index as i64),
            Value::string(name.clone()),
        );
        let values = if let Some(values) = attrs.get(&name) {
            ldap_text_values_array(values)
        } else {
            ldap_binary_values_array(bin_attrs.get(&name).into_iter().flatten())
        };
        output.insert(
            crate::ArrayKey::String(crate::PhpString::from_bytes(name.into_bytes())),
            Value::Array(values),
        );
    }
    output
}

fn ldap_text_values_array(values: &[String]) -> PhpArray {
    let mut output = PhpArray::new();
    output.insert(
        crate::ArrayKey::String(crate::PhpString::from("count")),
        Value::Int(values.len() as i64),
    );
    for (index, value) in values.iter().enumerate() {
        output.insert(
            crate::ArrayKey::Int(index as i64),
            Value::string(value.clone()),
        );
    }
    output
}

fn ldap_binary_values_array<'a>(values: impl Iterator<Item = &'a Vec<u8>>) -> PhpArray {
    let values = values.collect::<Vec<_>>();
    let mut output = PhpArray::new();
    output.insert(
        crate::ArrayKey::String(crate::PhpString::from("count")),
        Value::Int(values.len() as i64),
    );
    for (index, value) in values.into_iter().enumerate() {
        output.insert(
            crate::ArrayKey::Int(index as i64),
            Value::String(crate::PhpString::from_bytes(value.clone())),
        );
    }
    output
}

struct FtpEntry {
    client: FtpStream,
    passive: bool,
    timeout: Duration,
    auto_seek: bool,
    use_pasv_address: bool,
}

impl std::fmt::Debug for FtpEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FtpEntry")
            .field("passive", &self.passive)
            .field("timeout", &self.timeout)
            .field("auto_seek", &self.auto_seek)
            .field("use_pasv_address", &self.use_pasv_address)
            .finish_non_exhaustive()
    }
}

impl FtpState {
    /// Opens a plain FTP control connection and validates the server greeting.
    pub fn connect(
        &mut self,
        host: &str,
        port: u16,
        timeout_secs: u64,
        allow_configured_live_endpoint: bool,
    ) -> Result<i64, i32> {
        let host = if allow_configured_live_endpoint {
            host.to_owned()
        } else {
            loopback_host(host).ok_or(libc::EACCES)?.to_owned()
        };
        let timeout = Duration::from_secs(timeout_secs.max(1));
        let address = (host.as_str(), port)
            .to_socket_addrs()
            .map_err(raw_errno)?
            .next()
            .ok_or(libc::ECONNREFUSED)?;
        let mut client =
            FtpStream::connect_timeout(address, timeout).map_err(|_| libc::ECONNREFUSED)?;
        let _ = client.get_ref().set_read_timeout(Some(timeout));
        let _ = client.get_ref().set_write_timeout(Some(timeout));
        client.set_mode(Mode::Passive);
        client.set_passive_nat_workaround(true);
        let id = if self.next_id <= 0 { 1 } else { self.next_id };
        self.next_id = id.saturating_add(1);
        self.connections.insert(
            id,
            FtpEntry {
                client,
                passive: false,
                timeout,
                auto_seek: true,
                use_pasv_address: true,
            },
        );
        Ok(id)
    }

    /// Authenticates with USER/PASS.
    pub fn login(&mut self, id: i64, user: &str, password: &str) -> Result<bool, i32> {
        let entry = self.connection_mut(id)?;
        Ok(entry.client.login(user, password).is_ok())
    }

    /// Returns the current remote directory from PWD.
    pub fn pwd(&mut self, id: i64) -> Result<Option<String>, i32> {
        let entry = self.connection_mut(id)?;
        Ok(entry.client.pwd().ok())
    }

    /// Changes the remote directory with CWD.
    pub fn chdir(&mut self, id: i64, path: &str) -> Result<bool, i32> {
        let entry = self.connection_mut(id)?;
        Ok(entry.client.cwd(path).is_ok())
    }

    /// Changes to the parent directory with CDUP.
    pub fn cdup(&mut self, id: i64) -> Result<bool, i32> {
        let entry = self.connection_mut(id)?;
        Ok(entry.client.cdup().is_ok())
    }

    /// Runs an EXEC command on servers that support SITE EXEC.
    pub fn exec(&mut self, id: i64, command: &str) -> Result<bool, i32> {
        let entry = self.connection_mut(id)?;
        Ok(entry.client.site(format!("EXEC {command}")).is_ok())
    }

    /// Sends a raw FTP command and returns response lines without CRLF.
    pub fn raw(&mut self, id: i64, command: &str) -> Result<Vec<String>, i32> {
        let entry = self.connection_mut(id)?;
        let response = entry
            .client
            .custom_command(command, &RAW_COMMAND_OK_STATUSES)
            .map_err(|_| libc::EIO)?;
        Ok(vec![ftp_response_line(response)])
    }

    /// Creates a remote directory and returns the path reported by the server.
    pub fn mkdir(&mut self, id: i64, path: &str) -> Result<Option<String>, i32> {
        let entry = self.connection_mut(id)?;
        Ok(entry.client.mkdir(path).is_ok().then(|| path.to_owned()))
    }

    /// Removes a remote directory.
    pub fn rmdir(&mut self, id: i64, path: &str) -> Result<bool, i32> {
        let entry = self.connection_mut(id)?;
        Ok(entry.client.rmdir(path).is_ok())
    }

    /// Deletes a remote file.
    pub fn delete(&mut self, id: i64, path: &str) -> Result<bool, i32> {
        let entry = self.connection_mut(id)?;
        Ok(entry.client.rm(path).is_ok())
    }

    /// Renames a remote path through RNFR/RNTO.
    pub fn rename(&mut self, id: i64, from: &str, to: &str) -> Result<bool, i32> {
        let entry = self.connection_mut(id)?;
        Ok(entry.client.rename(from, to).is_ok())
    }

    /// Sends a SITE command.
    pub fn site(&mut self, id: i64, command: &str) -> Result<bool, i32> {
        let entry = self.connection_mut(id)?;
        Ok(entry.client.site(command).is_ok())
    }

    /// Sends an ALLO command and returns the server response line.
    pub fn alloc(&mut self, id: i64, size: i64) -> Result<(bool, Option<String>), i32> {
        let entry = self.connection_mut(id)?;
        match entry.client.custom_command(
            format!("ALLO {size}"),
            &[Status::CommandOk, Status::CommandNotImplemented],
        ) {
            Ok(response) => Ok((true, Some(ftp_response_line(response)))),
            Err(_) => Ok((false, None)),
        }
    }

    /// Sends SITE CHMOD and returns the permission on success.
    pub fn chmod(&mut self, id: i64, permissions: i64, path: &str) -> Result<Option<i64>, i32> {
        let entry = self.connection_mut(id)?;
        Ok(entry
            .client
            .site(format!("CHMOD {permissions:o} {path}"))
            .is_ok()
            .then_some(permissions))
    }

    /// Returns the server system type from SYST.
    pub fn systype(&mut self, id: i64) -> Result<Option<String>, i32> {
        let entry = self.connection_mut(id)?;
        Ok(entry
            .client
            .custom_command("SYST", &[Status::Name])
            .ok()
            .and_then(|response| response.as_string().ok())
            .map(|line| strip_ftp_status_prefix(&line).to_owned()))
    }

    /// Returns SIZE, or -1 when the server does not return a numeric 213 value.
    pub fn size(&mut self, id: i64, path: &str) -> Result<i64, i32> {
        let entry = self.connection_mut(id)?;
        Ok(entry
            .client
            .size(path)
            .ok()
            .and_then(|size| i64::try_from(size).ok())
            .unwrap_or(-1))
    }

    /// Returns MDTM, or -1 when the server does not return a numeric 213 value.
    pub fn mdtm(&mut self, id: i64, path: &str) -> Result<i64, i32> {
        let entry = self.connection_mut(id)?;
        Ok(entry
            .client
            .mdtm(path)
            .ok()
            .and_then(|value| value.format("%Y%m%d%H%M%S").to_string().parse().ok())
            .unwrap_or(-1))
    }

    /// Enables or disables passive data-channel mode.
    pub fn set_passive(&mut self, id: i64, enabled: bool) -> Result<bool, i32> {
        let entry = self.connection_mut(id)?;
        entry.passive = enabled;
        entry
            .client
            .set_mode(if enabled { Mode::Passive } else { Mode::Active });
        Ok(true)
    }

    /// Reads an NLST response through a passive data connection.
    pub fn nlist(&mut self, id: i64, path: &str) -> Result<Option<Vec<String>>, i32> {
        let entry = self.connection_mut(id)?;
        if !entry.passive {
            return Ok(None);
        }
        Ok(entry.client.nlst(non_empty_path(path)).ok())
    }

    /// Reads a LIST response through a passive data connection.
    pub fn rawlist(
        &mut self,
        id: i64,
        path: &str,
        recursive: bool,
    ) -> Result<Option<Vec<String>>, i32> {
        let entry = self.connection_mut(id)?;
        if !entry.passive {
            return Ok(None);
        }
        let path = if recursive {
            format!("-R {path}")
        } else {
            path.to_owned()
        };
        Ok(entry.client.list(non_empty_path(&path)).ok())
    }

    /// Reads an MLSD response through a passive data connection.
    pub fn mlsd(&mut self, id: i64, path: &str) -> Result<Option<Vec<String>>, i32> {
        let entry = self.connection_mut(id)?;
        if !entry.passive {
            return Ok(None);
        }
        Ok(entry.client.mlsd(non_empty_path(path)).ok())
    }

    /// Retrieves a remote file through a passive data connection.
    pub fn retrieve(
        &mut self,
        id: i64,
        path: &str,
        mode: i64,
        offset: i64,
    ) -> Result<Option<Vec<u8>>, i32> {
        let entry = self.connection_mut(id)?;
        if !entry.passive {
            return Ok(None);
        }
        set_suppaftp_transfer_type(entry, mode)?;
        if offset > 0 {
            let offset = usize::try_from(offset).map_err(|_| libc::EINVAL)?;
            entry
                .client
                .resume_transfer(offset)
                .map_err(|_| libc::EIO)?;
        }
        Ok(entry
            .client
            .retr_as_buffer(path)
            .ok()
            .map(Cursor::into_inner))
    }

    /// Stores a remote file through a passive data connection.
    pub fn store(
        &mut self,
        id: i64,
        path: &str,
        bytes: &[u8],
        mode: i64,
        offset: i64,
        append: bool,
    ) -> Result<bool, i32> {
        let entry = self.connection_mut(id)?;
        if !entry.passive {
            return Ok(false);
        }
        set_suppaftp_transfer_type(entry, mode)?;
        if offset > 0 {
            let offset = usize::try_from(offset).map_err(|_| libc::EINVAL)?;
            entry
                .client
                .resume_transfer(offset)
                .map_err(|_| libc::EIO)?;
        }
        let mut reader = Cursor::new(bytes);
        let result = if append {
            entry.client.append_file(path, &mut reader)
        } else {
            entry.client.put_file(path, &mut reader)
        };
        Ok(result.is_ok())
    }

    /// Returns one FTP option value.
    pub fn get_option(&mut self, id: i64, option: i64) -> Result<Option<FtpOptionValue>, i32> {
        let entry = self.connection_mut(id)?;
        match option {
            0 => Ok(Some(FtpOptionValue::Int(
                i64::try_from(entry.timeout.as_secs()).unwrap_or(i64::MAX),
            ))),
            1 => Ok(Some(FtpOptionValue::Bool(entry.auto_seek))),
            2 => Ok(Some(FtpOptionValue::Bool(entry.use_pasv_address))),
            _ => Ok(None),
        }
    }

    /// Updates one FTP option value.
    pub fn set_option(&mut self, id: i64, option: i64, value: FtpOptionValue) -> Result<bool, i32> {
        let entry = self.connection_mut(id)?;
        match (option, value) {
            (0, FtpOptionValue::Int(seconds)) if seconds > 0 => {
                entry.timeout = Duration::from_secs(u64::try_from(seconds).unwrap_or(u64::MAX));
                let _ = entry.client.get_ref().set_read_timeout(Some(entry.timeout));
                let _ = entry
                    .client
                    .get_ref()
                    .set_write_timeout(Some(entry.timeout));
                Ok(true)
            }
            (1, FtpOptionValue::Bool(enabled)) => {
                entry.auto_seek = enabled;
                Ok(true)
            }
            (2, FtpOptionValue::Bool(enabled)) => {
                entry.use_pasv_address = enabled;
                entry.client.set_passive_nat_workaround(!enabled);
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Closes a connection. QUIT failures are intentionally non-fatal.
    pub fn close(&mut self, id: i64) -> Result<bool, i32> {
        let Some(mut entry) = self.connections.remove(&id) else {
            return Err(libc::EBADF);
        };
        let _ = entry.client.quit();
        Ok(true)
    }

    /// Returns whether an FTP ID is currently open.
    #[must_use]
    pub fn contains(&self, id: i64) -> bool {
        self.connections.contains_key(&id)
    }

    fn connection_mut(&mut self, id: i64) -> Result<&mut FtpEntry, i32> {
        self.connections.get_mut(&id).ok_or(libc::EBADF)
    }
}

/// FTP option values with PHP-visible scalar shape.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FtpOptionValue {
    Int(i64),
    Bool(bool),
}

/// Request-local state for the `sockets` extension.
#[derive(Debug, Default)]
pub struct SocketState {
    next_id: i64,
    last_error: i32,
    sockets: BTreeMap<i64, SocketEntry>,
    options: BTreeMap<i64, BTreeMap<(i64, i64), i64>>,
}

#[derive(Debug)]
enum SocketEntry {
    Created {
        domain: i64,
        socket_type: i64,
        protocol: i64,
    },
    Listener(TcpListener),
    Stream(TcpStream),
    #[cfg(unix)]
    UnixListener(UnixListener),
    #[cfg(unix)]
    UnixStream(UnixStream),
    Closed,
}

impl SocketState {
    /// Registers a newly-created socket placeholder and returns its stable ID.
    pub fn create(&mut self, domain: i64, socket_type: i64, protocol: i64) -> i64 {
        let id = if self.next_id <= 0 { 1 } else { self.next_id };
        self.next_id = id.saturating_add(1);
        self.sockets.insert(
            id,
            SocketEntry::Created {
                domain,
                socket_type,
                protocol,
            },
        );
        self.options.insert(id, BTreeMap::new());
        self.last_error = 0;
        id
    }

    /// Binds a stream listener to a loopback TCP address or Unix socket path.
    pub fn bind_stream_listener(&mut self, id: i64, address: &str, port: u16) -> Result<(), i32> {
        let Some(entry) = self.sockets.get_mut(&id) else {
            return Err(libc::EBADF);
        };
        let SocketEntry::Created {
            domain,
            socket_type,
            protocol,
        } = entry
        else {
            return Err(libc::EINVAL);
        };
        if *socket_type != i64::from(libc::SOCK_STREAM) {
            return Err(libc::EAFNOSUPPORT);
        }
        if *domain == i64::from(libc::AF_UNIX) {
            if *protocol != 0 {
                return Err(libc::EPROTONOSUPPORT);
            }
            return bind_unix_listener(entry, address);
        }
        if *domain != i64::from(libc::AF_INET)
            || (*protocol != 0 && *protocol != i64::from(libc::IPPROTO_TCP))
        {
            return Err(libc::EAFNOSUPPORT);
        }
        let bind_address = if address == "localhost" {
            "127.0.0.1"
        } else {
            address
        };
        if bind_address != "127.0.0.1" {
            return Err(libc::EACCES);
        }
        match TcpListener::bind((bind_address, port)) {
            Ok(listener) => {
                *entry = SocketEntry::Listener(listener);
                self.last_error = 0;
                Ok(())
            }
            Err(error) => Err(raw_errno(error)),
        }
    }

    /// Marks a listener ready. `TcpListener::bind` already starts listening.
    pub fn listen(&mut self, id: i64) -> Result<(), i32> {
        match self.sockets.get(&id) {
            Some(SocketEntry::Listener(_)) => {
                self.last_error = 0;
                Ok(())
            }
            #[cfg(unix)]
            Some(SocketEntry::UnixListener(_)) => {
                self.last_error = 0;
                Ok(())
            }
            Some(_) => Err(libc::EINVAL),
            None => Err(libc::EBADF),
        }
    }

    /// Connects a stream socket to a loopback TCP listener or Unix socket path.
    pub fn connect_stream(&mut self, id: i64, address: &str, port: u16) -> Result<(), i32> {
        let Some(entry) = self.sockets.get_mut(&id) else {
            return Err(libc::EBADF);
        };
        let SocketEntry::Created {
            domain,
            socket_type,
            protocol,
        } = entry
        else {
            return Err(libc::EINVAL);
        };
        if *socket_type != i64::from(libc::SOCK_STREAM) {
            return Err(libc::EAFNOSUPPORT);
        }
        if *domain == i64::from(libc::AF_UNIX) {
            if *protocol != 0 {
                return Err(libc::EPROTONOSUPPORT);
            }
            return connect_unix_stream(entry, address);
        }
        if *domain != i64::from(libc::AF_INET)
            || (*protocol != 0 && *protocol != i64::from(libc::IPPROTO_TCP))
        {
            return Err(libc::EAFNOSUPPORT);
        }
        let connect_address = if address == "localhost" {
            "127.0.0.1"
        } else {
            address
        };
        if connect_address != "127.0.0.1" {
            return Err(libc::EACCES);
        }
        match TcpStream::connect((connect_address, port)) {
            Ok(stream) => {
                *entry = SocketEntry::Stream(stream);
                self.last_error = 0;
                Ok(())
            }
            Err(error) => Err(raw_errno(error)),
        }
    }

    /// Accepts one connection from a listener and returns a new socket ID.
    pub fn accept(&mut self, id: i64) -> Result<i64, i32> {
        match self.sockets.get(&id) {
            Some(SocketEntry::Listener(listener)) => match listener.accept() {
                Ok((stream, _addr)) => self.insert_accepted(SocketEntry::Stream(stream)),
                Err(error) => Err(raw_errno(error)),
            },
            #[cfg(unix)]
            Some(SocketEntry::UnixListener(listener)) => match listener.accept() {
                Ok((stream, _addr)) => self.insert_accepted(SocketEntry::UnixStream(stream)),
                Err(error) => Err(raw_errno(error)),
            },
            Some(_) => Err(libc::EINVAL),
            None => Err(libc::EBADF),
        }
    }

    fn insert_accepted(&mut self, entry: SocketEntry) -> Result<i64, i32> {
        let id = if self.next_id <= 0 { 1 } else { self.next_id };
        self.next_id = id.saturating_add(1);
        self.sockets.insert(id, entry);
        self.options.insert(id, BTreeMap::new());
        self.last_error = 0;
        Ok(id)
    }

    /// Writes bytes to a connected stream.
    pub fn write(&mut self, id: i64, bytes: &[u8]) -> Result<usize, i32> {
        match self.sockets.get_mut(&id) {
            Some(SocketEntry::Stream(stream)) => match stream.write(bytes) {
                Ok(written) => {
                    self.last_error = 0;
                    Ok(written)
                }
                Err(error) => Err(raw_errno(error)),
            },
            #[cfg(unix)]
            Some(SocketEntry::UnixStream(stream)) => match stream.write(bytes) {
                Ok(written) => {
                    self.last_error = 0;
                    Ok(written)
                }
                Err(error) => Err(raw_errno(error)),
            },
            Some(_) => Err(libc::EINVAL),
            None => Err(libc::EBADF),
        }
    }

    /// Reads up to `length` bytes from a connected stream.
    pub fn read(&mut self, id: i64, length: usize) -> Result<Vec<u8>, i32> {
        match self.sockets.get_mut(&id) {
            Some(SocketEntry::Stream(stream)) => {
                let mut buffer = vec![0; length];
                match stream.read(&mut buffer) {
                    Ok(read) => {
                        buffer.truncate(read);
                        self.last_error = 0;
                        Ok(buffer)
                    }
                    Err(error) => Err(raw_errno(error)),
                }
            }
            #[cfg(unix)]
            Some(SocketEntry::UnixStream(stream)) => {
                let mut buffer = vec![0; length];
                match stream.read(&mut buffer) {
                    Ok(read) => {
                        buffer.truncate(read);
                        self.last_error = 0;
                        Ok(buffer)
                    }
                    Err(error) => Err(raw_errno(error)),
                }
            }
            Some(_) => Err(libc::EINVAL),
            None => Err(libc::EBADF),
        }
    }

    /// Returns the local socket name for a bound or connected socket.
    #[must_use]
    pub fn local_name(&self, id: i64) -> Option<(String, Option<u16>)> {
        match self.sockets.get(&id)? {
            SocketEntry::Listener(listener) => tcp_name(listener.local_addr().ok()),
            SocketEntry::Stream(stream) => tcp_name(stream.local_addr().ok()),
            #[cfg(unix)]
            SocketEntry::UnixListener(listener) => unix_name(listener.local_addr().ok()),
            #[cfg(unix)]
            SocketEntry::UnixStream(stream) => unix_name(stream.local_addr().ok()),
            SocketEntry::Created { .. } | SocketEntry::Closed => None,
        }
    }

    /// Returns the peer socket name for a connected stream socket.
    #[must_use]
    pub fn peer_name(&self, id: i64) -> Option<(String, Option<u16>)> {
        match self.sockets.get(&id)? {
            SocketEntry::Stream(stream) => tcp_name(stream.peer_addr().ok()),
            #[cfg(unix)]
            SocketEntry::UnixStream(stream) => unix_name(stream.peer_addr().ok()),
            SocketEntry::Created { .. } | SocketEntry::Listener(_) | SocketEntry::Closed => None,
            #[cfg(unix)]
            SocketEntry::UnixListener(_) => None,
        }
    }

    /// Shuts down one or both halves of a stream socket.
    pub fn shutdown(&mut self, id: i64, mode: i64) -> Result<(), i32> {
        let shutdown = match mode {
            0 => Shutdown::Read,
            1 => Shutdown::Write,
            2 => Shutdown::Both,
            _ => return Err(libc::EINVAL),
        };
        match self.sockets.get_mut(&id) {
            Some(SocketEntry::Stream(stream)) => match stream.shutdown(shutdown) {
                Ok(()) => {
                    self.last_error = 0;
                    Ok(())
                }
                Err(error) => Err(raw_errno(error)),
            },
            #[cfg(unix)]
            Some(SocketEntry::UnixStream(stream)) => match stream.shutdown(shutdown) {
                Ok(()) => {
                    self.last_error = 0;
                    Ok(())
                }
                Err(error) => Err(raw_errno(error)),
            },
            Some(_) => Err(libc::EINVAL),
            None => Err(libc::EBADF),
        }
    }

    /// Sets a supported socket option on a live socket handle.
    pub fn set_option(&mut self, id: i64, level: i64, option: i64, value: i64) -> Result<(), i32> {
        if !is_supported_socket_option(level, option) {
            return Err(libc::ENOPROTOOPT);
        }
        match self.sockets.get_mut(&id) {
            Some(SocketEntry::Stream(stream))
                if level == i64::from(libc::IPPROTO_TCP)
                    && option == i64::from(libc::TCP_NODELAY) =>
            {
                stream.set_nodelay(value != 0).map_err(raw_errno)?;
            }
            Some(
                SocketEntry::Created { .. } | SocketEntry::Listener(_) | SocketEntry::Stream(_),
            ) => {}
            #[cfg(unix)]
            Some(SocketEntry::UnixListener(_) | SocketEntry::UnixStream(_)) => {}
            Some(SocketEntry::Closed) => return Err(libc::EBADF),
            None => return Err(libc::EBADF),
        }
        self.options
            .entry(id)
            .or_default()
            .insert((level, option), i64::from(value != 0));
        self.last_error = 0;
        Ok(())
    }

    /// Returns a supported socket option from the live handle or stored state.
    pub fn option(&self, id: i64, level: i64, option: i64) -> Result<i64, i32> {
        if !is_supported_socket_option(level, option) {
            return Err(libc::ENOPROTOOPT);
        }
        match self.sockets.get(&id) {
            Some(SocketEntry::Stream(stream))
                if level == i64::from(libc::IPPROTO_TCP)
                    && option == i64::from(libc::TCP_NODELAY) =>
            {
                return stream.nodelay().map(i64::from).map_err(raw_errno);
            }
            Some(
                SocketEntry::Created { .. } | SocketEntry::Listener(_) | SocketEntry::Stream(_),
            ) => {}
            #[cfg(unix)]
            Some(SocketEntry::UnixListener(_) | SocketEntry::UnixStream(_)) => {}
            Some(SocketEntry::Closed) => return Err(libc::EBADF),
            None => return Err(libc::EBADF),
        }
        Ok(self
            .options
            .get(&id)
            .and_then(|options| options.get(&(level, option)).copied())
            .unwrap_or(0))
    }

    /// Closes a socket ID.
    pub fn close(&mut self, id: i64) -> Result<(), i32> {
        match self.sockets.get_mut(&id) {
            Some(entry) => {
                *entry = SocketEntry::Closed;
                self.last_error = 0;
                Ok(())
            }
            None => Err(libc::EBADF),
        }
    }

    /// Updates the last sockets error.
    pub fn set_last_error(&mut self, error: i32) {
        self.last_error = error;
    }

    /// Returns the last sockets error.
    #[must_use]
    pub const fn last_error(&self) -> i32 {
        self.last_error
    }
}

fn is_supported_socket_option(level: i64, option: i64) -> bool {
    matches!(
        (level as i32, option as i32),
        (libc::SOL_SOCKET, libc::SO_REUSEADDR)
            | (libc::SOL_SOCKET, libc::SO_KEEPALIVE)
            | (libc::IPPROTO_TCP, libc::TCP_NODELAY)
    )
}

fn tcp_name(addr: Option<SocketAddr>) -> Option<(String, Option<u16>)> {
    addr.map(|addr| (addr.ip().to_string(), Some(addr.port())))
}

#[cfg(unix)]
fn bind_unix_listener(entry: &mut SocketEntry, path: &str) -> Result<(), i32> {
    if path.is_empty() {
        return Err(libc::EINVAL);
    }
    match UnixListener::bind(path) {
        Ok(listener) => {
            *entry = SocketEntry::UnixListener(listener);
            Ok(())
        }
        Err(error) => Err(raw_errno(error)),
    }
}

#[cfg(not(unix))]
fn bind_unix_listener(_entry: &mut SocketEntry, _path: &str) -> Result<(), i32> {
    Err(libc::EAFNOSUPPORT)
}

#[cfg(unix)]
fn connect_unix_stream(entry: &mut SocketEntry, path: &str) -> Result<(), i32> {
    if path.is_empty() {
        return Err(libc::EINVAL);
    }
    match UnixStream::connect(path) {
        Ok(stream) => {
            *entry = SocketEntry::UnixStream(stream);
            Ok(())
        }
        Err(error) => Err(raw_errno(error)),
    }
}

#[cfg(not(unix))]
fn connect_unix_stream(_entry: &mut SocketEntry, _path: &str) -> Result<(), i32> {
    Err(libc::EAFNOSUPPORT)
}

#[cfg(unix)]
fn unix_name(addr: Option<std::os::unix::net::SocketAddr>) -> Option<(String, Option<u16>)> {
    let addr = addr?;
    let path = addr.as_pathname()?;
    Some((path.to_string_lossy().into_owned(), None))
}

fn raw_errno(error: std::io::Error) -> i32 {
    error.raw_os_error().unwrap_or(libc::EIO)
}

const RAW_COMMAND_OK_STATUSES: [Status; 10] = [
    Status::CommandOk,
    Status::CommandNotImplemented,
    Status::System,
    Status::Directory,
    Status::File,
    Status::Help,
    Status::Name,
    Status::RequestedFileActionOk,
    Status::PathCreated,
    Status::RequestFilePending,
];

fn loopback_host(host: &str) -> Option<&'static str> {
    match host {
        "127.0.0.1" | "localhost" => Some("127.0.0.1"),
        "::1" => Some("::1"),
        _ => None,
    }
}

fn set_suppaftp_transfer_type(entry: &mut FtpEntry, mode: i64) -> Result<(), i32> {
    let file_type = match mode {
        1 => FileType::Ascii(FormatControl::Default),
        2 => FileType::Binary,
        _ => return Err(libc::EINVAL),
    };
    entry.client.transfer_type(file_type).map_err(|_| libc::EIO)
}

fn non_empty_path(path: &str) -> Option<&str> {
    (!path.is_empty()).then_some(path)
}

fn ftp_response_line(response: Response) -> String {
    let body = response.as_string().unwrap_or_default();
    let code = response.status as u32;
    let code_prefix = format!("{code} ");
    if body.is_empty() {
        code.to_string()
    } else if body.starts_with(&code_prefix) {
        body
    } else {
        format!("{code} {body}")
    }
}

fn strip_ftp_status_prefix(line: &str) -> &str {
    if line.len() > 4
        && line.as_bytes()[..3]
            .iter()
            .all(|byte| byte.is_ascii_digit())
        && line.as_bytes()[3] == b' '
    {
        &line[4..]
    } else {
        line
    }
}

pub(in crate::builtins) const JSON_ERROR_NONE: i64 = 0;
pub(in crate::builtins) const JSON_ERROR_DEPTH: i64 = 1;
pub(in crate::builtins) const JSON_ERROR_STATE_MISMATCH: i64 = 2;
pub(in crate::builtins) const JSON_ERROR_CTRL_CHAR: i64 = 3;
pub(in crate::builtins) const JSON_ERROR_SYNTAX: i64 = 4;
pub(in crate::builtins) const JSON_ERROR_UTF8: i64 = 5;
pub const JSON_ERROR_RECURSION: i64 = 6;
pub(in crate::builtins) const JSON_ERROR_INF_OR_NAN: i64 = 7;
pub(in crate::builtins) const JSON_ERROR_UNSUPPORTED_TYPE: i64 = 8;
pub(in crate::builtins) const JSON_ERROR_INVALID_PROPERTY_NAME: i64 = 9;
pub(in crate::builtins) const JSON_ERROR_UTF16: i64 = 10;
pub(in crate::builtins) const JSON_ERROR_NON_BACKED_ENUM: i64 = 11;
pub(in crate::builtins) const JSON_OBJECT_AS_ARRAY: i64 = 1;
pub(in crate::builtins) const JSON_BIGINT_AS_STRING: i64 = 2;
pub(in crate::builtins) const JSON_HEX_TAG: i64 = 1;
pub(in crate::builtins) const JSON_HEX_AMP: i64 = 2;
pub(in crate::builtins) const JSON_HEX_APOS: i64 = 4;
pub(in crate::builtins) const JSON_HEX_QUOT: i64 = 8;
pub(in crate::builtins) const JSON_FORCE_OBJECT: i64 = 16;
pub(in crate::builtins) const JSON_NUMERIC_CHECK: i64 = 32;
pub(in crate::builtins) const JSON_PRETTY_PRINT: i64 = 128;
pub const JSON_PARTIAL_OUTPUT_ON_ERROR: i64 = 512;
pub(in crate::builtins) const JSON_UNESCAPED_SLASHES: i64 = 64;
pub(in crate::builtins) const JSON_UNESCAPED_UNICODE: i64 = 256;
pub(in crate::builtins) const JSON_PRESERVE_ZERO_FRACTION: i64 = 1024;
pub(in crate::builtins) const JSON_UNESCAPED_LINE_TERMINATORS: i64 = 2048;
pub(in crate::builtins) const JSON_INVALID_UTF8_IGNORE: i64 = 1_048_576;
pub(in crate::builtins) const JSON_INVALID_UTF8_SUBSTITUTE: i64 = 2_097_152;
pub const JSON_THROW_ON_ERROR: i64 = 4_194_304;

/// Request-local OPcache facade state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OpcacheState {
    compiled_scripts: BTreeSet<String>,
    compile_attempts: u64,
    invalidations: u64,
    resets: u64,
}

impl OpcacheState {
    /// Records a script as compiled by the facade.
    pub fn compile_script(&mut self, path: impl Into<String>) {
        self.compile_attempts = self.compile_attempts.saturating_add(1);
        self.compiled_scripts.insert(path.into());
    }

    /// Removes one script from the facade cache.
    pub fn invalidate_script(&mut self, path: &str) -> bool {
        self.invalidations = self.invalidations.saturating_add(1);
        self.compiled_scripts.remove(path)
    }

    /// Clears all request-local OPcache facade state.
    pub fn reset(&mut self) {
        self.resets = self.resets.saturating_add(1);
        self.compiled_scripts.clear();
    }

    /// Returns true when the script has been compiled in this request.
    #[must_use]
    pub fn is_script_cached(&self, path: &str) -> bool {
        self.compiled_scripts.contains(path)
    }

    /// Compiled script paths in deterministic order.
    pub fn compiled_scripts(&self) -> impl Iterator<Item = &str> {
        self.compiled_scripts.iter().map(String::as_str)
    }

    /// Number of successful compile-file calls recorded by this request.
    #[must_use]
    pub const fn compile_attempts(&self) -> u64 {
        self.compile_attempts
    }

    /// Number of invalidate calls recorded by this request.
    #[must_use]
    pub const fn invalidations(&self) -> u64 {
        self.invalidations
    }

    /// Number of reset calls recorded by this request.
    #[must_use]
    pub const fn resets(&self) -> u64 {
        self.resets
    }
}

/// Request-local SOAP facade state.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct SoapState {
    error_handler_enabled: bool,
}

impl SoapState {
    /// Returns whether the SOAP facade error handler is enabled.
    #[must_use]
    pub const fn error_handler_enabled(&self) -> bool {
        self.error_handler_enabled
    }

    /// Sets the SOAP facade error handler flag and returns the previous value.
    pub fn set_error_handler_enabled(&mut self, enabled: bool) -> bool {
        let previous = self.error_handler_enabled;
        self.error_handler_enabled = enabled;
        previous
    }
}

/// Request-local gettext binding and codeset state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GettextState {
    current_domain: String,
    domain_paths: BTreeMap<String, String>,
    domain_codesets: BTreeMap<String, String>,
}

impl Default for GettextState {
    fn default() -> Self {
        Self {
            current_domain: "messages".to_owned(),
            domain_paths: BTreeMap::new(),
            domain_codesets: BTreeMap::new(),
        }
    }
}

impl GettextState {
    /// Current text domain.
    #[must_use]
    pub fn current_domain(&self) -> &str {
        &self.current_domain
    }

    /// Updates the current text domain and returns the new value.
    pub fn set_domain(&mut self, domain: impl Into<String>) -> &str {
        self.current_domain = domain.into();
        &self.current_domain
    }

    /// Reads a bound catalog directory for a domain.
    #[must_use]
    pub fn domain_path(&self, domain: &str) -> Option<&str> {
        self.domain_paths.get(domain).map(String::as_str)
    }

    /// Binds a catalog directory for a domain.
    pub fn bind_domain_path(&mut self, domain: impl Into<String>, path: impl Into<String>) -> &str {
        let domain = domain.into();
        self.domain_paths.insert(domain.clone(), path.into());
        self.domain_paths
            .get(&domain)
            .expect("domain path inserted")
            .as_str()
    }

    /// Reads a bound output codeset for a domain.
    #[must_use]
    pub fn domain_codeset(&self, domain: &str) -> Option<&str> {
        self.domain_codesets.get(domain).map(String::as_str)
    }

    /// Binds an output codeset for a domain.
    pub fn bind_domain_codeset(
        &mut self,
        domain: impl Into<String>,
        codeset: impl Into<String>,
    ) -> &str {
        let domain = domain.into();
        self.domain_codesets.insert(domain.clone(), codeset.into());
        self.domain_codesets
            .get(&domain)
            .expect("domain codeset inserted")
            .as_str()
    }
}

/// Host System V shared-memory backend for `shmop`.
#[derive(Debug)]
pub struct ShmopState {
    next_id: i64,
    segments: BTreeMap<i64, ShmopSegment>,
    keyed_segments: BTreeMap<i64, i64>,
}

impl Default for ShmopState {
    fn default() -> Self {
        Self {
            next_id: 1,
            segments: BTreeMap::new(),
            keyed_segments: BTreeMap::new(),
        }
    }
}

#[derive(Debug)]
pub struct ShmopSegment {
    key: Option<i64>,
    shmid: libc::c_int,
    addr: usize,
    size: usize,
    deleted: bool,
}

#[allow(unsafe_code)] // direct SysV shared-memory mapping access, bounds checked
impl ShmopSegment {
    fn new(key: Option<i64>, shmid: libc::c_int, addr: usize, size: usize) -> Self {
        Self {
            key,
            shmid,
            addr,
            size,
            deleted: false,
        }
    }

    /// Segment byte length.
    #[must_use]
    pub fn size(&self) -> usize {
        self.size
    }

    /// Reads a binary-safe range from the segment.
    #[must_use]
    pub fn read(&self, offset: usize, size: usize) -> Vec<u8> {
        let end = offset.saturating_add(size).min(self.size);
        let read = end.saturating_sub(offset);
        if read == 0 {
            return Vec::new();
        }
        // SAFETY: `addr` is a live `shmat` mapping owned by this segment and
        // `read` is clamped to the segment length above.
        unsafe { std::slice::from_raw_parts((self.addr as *const u8).add(offset), read).to_vec() }
    }

    /// Writes bytes into the segment and returns the count written.
    pub fn write(&mut self, offset: usize, data: &[u8]) -> usize {
        let end = offset.saturating_add(data.len()).min(self.size);
        let written = end.saturating_sub(offset);
        if written != 0 {
            // SAFETY: `addr` is a live writable `shmat` mapping for this
            // segment and `written` is clamped to the segment length above.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    data.as_ptr(),
                    (self.addr as *mut u8).add(offset),
                    written,
                );
            }
        }
        written
    }

    /// Marks the segment deleted while existing handles may still read it.
    pub fn delete(&mut self) {
        // SAFETY: direct SysV IPC call; the result is intentionally ignored
        // here because PHP exposes deletion as a best-effort boolean and the
        // caller already checked that this segment is live.
        unsafe {
            libc::shmctl(self.shmid, libc::IPC_RMID, std::ptr::null_mut());
        }
        self.deleted = true;
    }

    /// Whether this segment has been deleted.
    #[must_use]
    pub const fn is_deleted(&self) -> bool {
        self.deleted
    }
}

#[allow(unsafe_code)] // direct SysV shared-memory detach during resource drop
impl Drop for ShmopSegment {
    fn drop(&mut self) {
        if self.addr != 0 {
            // SAFETY: `addr` was returned by `shmat` for this segment. Detach
            // is idempotent at the OS resource level for this mapping owner.
            unsafe {
                libc::shmdt(self.addr as *const libc::c_void);
            }
            self.addr = 0;
        }
    }
}

impl ShmopState {
    /// Opens or creates a System V shared-memory segment. Key `0` creates
    /// private segments.
    pub fn open(&mut self, key: i64, mode: char, permissions: i64, size: usize) -> Option<i64> {
        let keyed_id = (key != 0)
            .then(|| self.keyed_segments.get(&key).copied())
            .flatten();
        match mode {
            'a' | 'w' => keyed_id
                .filter(|id| self.segment(*id).is_some())
                .or_else(|| self.attach_existing_segment(key, mode == 'a')),
            'c' => keyed_id
                .filter(|id| self.segment(*id).is_some())
                .or_else(|| {
                    self.create_segment((key != 0).then_some(key), mode, permissions, size)
                }),
            'n' => {
                if keyed_id.is_some_and(|id| self.segment(id).is_some()) {
                    None
                } else {
                    self.create_segment((key != 0).then_some(key), mode, permissions, size)
                }
            }
            _ => None,
        }
    }

    /// Returns an existing live segment.
    #[must_use]
    pub fn segment(&self, id: i64) -> Option<&ShmopSegment> {
        self.segments
            .get(&id)
            .filter(|segment| !segment.is_deleted())
    }

    /// Returns an existing live segment mutably.
    pub fn segment_mut(&mut self, id: i64) -> Option<&mut ShmopSegment> {
        self.segments
            .get_mut(&id)
            .filter(|segment| !segment.is_deleted())
    }

    /// Marks a segment deleted and removes its keyed lookup.
    pub fn delete(&mut self, id: i64) -> bool {
        let Some(segment) = self.segments.get_mut(&id) else {
            return false;
        };
        if segment.is_deleted() {
            return false;
        }
        if let Some(key) = segment.key {
            self.keyed_segments.remove(&key);
        }
        segment.delete();
        true
    }

    fn attach_existing_segment(&mut self, key: i64, read_only: bool) -> Option<i64> {
        if key == 0 {
            return None;
        }
        let shmid = shmop_shmget(key as libc::key_t, 1, 0).ok()?;
        let size = shmop_segment_size(shmid)?;
        let addr = shmop_attach(shmid, read_only).ok()?;
        let id = self.next_id;
        self.next_id += 1;
        self.segments
            .insert(id, ShmopSegment::new(Some(key), shmid, addr, size));
        self.keyed_segments.insert(key, id);
        Some(id)
    }

    fn create_segment(
        &mut self,
        key: Option<i64>,
        mode: char,
        permissions: i64,
        size: usize,
    ) -> Option<i64> {
        let key_t = key
            .map(|key| key as libc::key_t)
            .unwrap_or(libc::IPC_PRIVATE);
        let permissions = permissions as libc::c_int;
        let shmid = match mode {
            'n' => {
                shmop_shmget(key_t, size, libc::IPC_CREAT | libc::IPC_EXCL | permissions).ok()?
            }
            'c' => {
                if key.is_some() {
                    shmop_shmget(key_t, 1, 0)
                        .or_else(|_| shmop_shmget(key_t, size, libc::IPC_CREAT | permissions))
                        .ok()?
                } else {
                    shmop_shmget(key_t, size, libc::IPC_CREAT | permissions).ok()?
                }
            }
            _ => return None,
        };
        let size = shmop_segment_size(shmid).unwrap_or(size);
        let addr = shmop_attach(shmid, false).ok()?;
        let id = self.next_id;
        self.next_id += 1;
        self.segments
            .insert(id, ShmopSegment::new(key, shmid, addr, size));
        if let Some(key) = key {
            self.keyed_segments.insert(key, id);
        }
        Some(id)
    }
}

#[allow(unsafe_code)] // direct SysV IPC call, result checked
fn shmop_shmget(key: libc::key_t, size: usize, flags: libc::c_int) -> Result<libc::c_int, i32> {
    // SAFETY: direct SysV IPC call; return value is checked for `-1`.
    let shmid = unsafe { libc::shmget(key, size, flags) };
    if shmid == -1 {
        Err(std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(libc::EIO))
    } else {
        Ok(shmid)
    }
}

#[allow(unsafe_code)] // direct SysV IPC attach, sentinel result checked
fn shmop_attach(shmid: libc::c_int, read_only: bool) -> Result<usize, i32> {
    let flags = if read_only { libc::SHM_RDONLY } else { 0 };
    // SAFETY: direct SysV IPC call; return value is checked against `(void*)-1`.
    let addr = unsafe { libc::shmat(shmid, std::ptr::null(), flags) };
    if addr as isize == -1 {
        Err(std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(libc::EIO))
    } else {
        Ok(addr as usize)
    }
}

#[allow(unsafe_code)] // direct SysV IPC metadata call, result checked
#[allow(clippy::useless_conversion)] // shm_segsz type varies by platform libc
fn shmop_segment_size(shmid: libc::c_int) -> Option<usize> {
    let mut stats = std::mem::MaybeUninit::<libc::shmid_ds>::zeroed();
    // SAFETY: `stats` points to valid writable storage for `IPC_STAT`.
    let result = unsafe { libc::shmctl(shmid, libc::IPC_STAT, stats.as_mut_ptr()) };
    if result == -1 {
        return None;
    }
    // SAFETY: `shmctl(IPC_STAT)` succeeded, so the kernel initialized `stats`.
    let stats = unsafe { stats.assume_init() };
    stats.shm_segsz.try_into().ok()
}

/// Request-local noninteractive readline state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadlineState {
    history: Vec<String>,
    info: BTreeMap<String, Value>,
    completion_callback: Option<String>,
    callback_handler: Option<ReadlineCallbackHandler>,
}

impl Default for ReadlineState {
    fn default() -> Self {
        let mut info = BTreeMap::new();
        info.insert("line_buffer".to_owned(), Value::string(""));
        info.insert("point".to_owned(), Value::Int(0));
        info.insert("end".to_owned(), Value::Int(0));
        info.insert("mark".to_owned(), Value::Int(0));
        info.insert("done".to_owned(), Value::Int(0));
        info.insert("pending_input".to_owned(), Value::Int(0));
        info.insert("prompt".to_owned(), Value::string(""));
        info.insert("terminal_name".to_owned(), Value::string(""));
        info.insert("completion_append_character".to_owned(), Value::string(" "));
        info.insert("completion_suppress_append".to_owned(), Value::Bool(false));
        info.insert("library_version".to_owned(), Value::string("8.2"));
        info.insert("readline_name".to_owned(), Value::string("other"));
        info.insert("attempted_completion_over".to_owned(), Value::Int(0));
        Self {
            history: Vec::new(),
            info,
            completion_callback: None,
            callback_handler: None,
        }
    }
}

/// Installed readline callback-handler metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadlineCallbackHandler {
    prompt: String,
    callback: String,
}

impl ReadlineState {
    /// Appends a history entry.
    pub fn add_history(&mut self, entry: impl Into<String>) {
        self.history.push(entry.into());
    }

    /// Clears request-local history.
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Returns request-local history entries.
    #[must_use]
    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// Replaces request-local history entries.
    pub fn set_history(&mut self, history: Vec<String>) {
        self.history = history;
    }

    /// Returns all readline info values.
    #[must_use]
    pub fn info(&self) -> &BTreeMap<String, Value> {
        &self.info
    }

    /// Returns one readline info value.
    #[must_use]
    pub fn info_value(&self, name: &str) -> Option<Value> {
        self.info.get(name).cloned()
    }

    /// Updates one readline info value and returns its previous value.
    pub fn set_info_value(&mut self, name: impl Into<String>, value: Value) -> Option<Value> {
        self.info.insert(name.into(), value)
    }

    /// Registers a completion callback.
    pub fn set_completion_callback(&mut self, callback: String) {
        self.completion_callback = Some(callback);
    }

    /// Installs a callback handler.
    pub fn install_callback_handler(&mut self, prompt: String, callback: String) {
        self.callback_handler = Some(ReadlineCallbackHandler { prompt, callback });
    }

    /// Removes a callback handler, returning whether one was installed.
    pub fn remove_callback_handler(&mut self) -> bool {
        self.callback_handler.take().is_some()
    }
}

/// System V message queue backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SysvMessageQueueState {
    next_id: i64,
    queues: BTreeMap<i64, SysvMessageQueue>,
    keyed_queues: BTreeMap<i64, i64>,
    object_queues: BTreeMap<u64, i64>,
}

impl Default for SysvMessageQueueState {
    fn default() -> Self {
        Self {
            next_id: 1,
            queues: BTreeMap::new(),
            keyed_queues: BTreeMap::new(),
            object_queues: BTreeMap::new(),
        }
    }
}

/// System V message queue metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SysvMessageQueue {
    key: i64,
    msqid: libc::c_int,
    permissions: i64,
    owner_uid: i64,
    owner_gid: i64,
    removed: bool,
    max_bytes: i64,
}

impl SysvMessageQueue {
    fn new(key: i64, msqid: libc::c_int, permissions: i64) -> Self {
        let stats = sysvmsg_stat(msqid).ok();
        Self {
            key,
            msqid,
            permissions,
            owner_uid: stats
                .as_ref()
                .map(|stats| stats.owner_uid)
                .unwrap_or_else(current_uid),
            owner_gid: stats
                .as_ref()
                .map(|stats| stats.owner_gid)
                .unwrap_or_else(current_gid),
            removed: false,
            max_bytes: stats
                .as_ref()
                .map(|stats| stats.max_bytes)
                .unwrap_or(16_384),
        }
    }

    /// Queue key.
    #[must_use]
    pub const fn key(&self) -> i64 {
        self.key
    }

    /// Queue permissions.
    #[must_use]
    pub const fn permissions(&self) -> i64 {
        self.permissions
    }

    /// Host message-queue id.
    #[must_use]
    pub const fn msqid(&self) -> libc::c_int {
        self.msqid
    }

    /// Updates queue permissions.
    pub fn set_permissions(&mut self, permissions: i64) {
        self.permissions = permissions;
    }

    /// Owner UID metadata.
    #[must_use]
    pub const fn owner_uid(&self) -> i64 {
        self.owner_uid
    }

    /// Updates owner UID metadata.
    pub fn set_owner_uid(&mut self, owner_uid: i64) {
        self.owner_uid = owner_uid;
    }

    /// Owner GID metadata.
    #[must_use]
    pub const fn owner_gid(&self) -> i64 {
        self.owner_gid
    }

    /// Updates owner GID metadata.
    pub fn set_owner_gid(&mut self, owner_gid: i64) {
        self.owner_gid = owner_gid;
    }

    /// Current pending message count.
    #[must_use]
    pub fn message_count(&self) -> usize {
        sysvmsg_stat(self.msqid)
            .map(|stats| stats.message_count)
            .unwrap_or(0)
    }

    /// Current pending payload byte count.
    #[must_use]
    pub fn byte_count(&self) -> usize {
        sysvmsg_stat(self.msqid)
            .map(|stats| stats.byte_count)
            .unwrap_or(0)
    }

    /// Maximum byte budget reported by queue metadata.
    #[must_use]
    pub const fn max_bytes(&self) -> i64 {
        self.max_bytes
    }

    /// Updates queue metadata byte budget.
    pub fn set_max_bytes(&mut self, max_bytes: i64) {
        self.max_bytes = max_bytes.max(0);
    }

    /// Applies queue settings through the host `msgctl(IPC_SET)` interface.
    pub fn apply_settings(
        &mut self,
        permissions: Option<i64>,
        owner_uid: Option<i64>,
        owner_gid: Option<i64>,
        max_bytes: Option<i64>,
    ) -> bool {
        let Ok(stats) =
            sysvmsg_apply_settings(self.msqid, permissions, owner_uid, owner_gid, max_bytes)
        else {
            return false;
        };
        self.permissions = stats.permissions;
        self.owner_uid = stats.owner_uid;
        self.owner_gid = stats.owner_gid;
        self.max_bytes = stats.max_bytes;
        true
    }
}

/// One queued System V message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SysvMessage {
    message_type: i64,
    payload: Vec<u8>,
    serialized: bool,
}

impl SysvMessage {
    /// Creates one queued message.
    #[must_use]
    pub fn new(message_type: i64, payload: Vec<u8>, serialized: bool) -> Self {
        Self {
            message_type,
            payload,
            serialized,
        }
    }

    /// Message type.
    #[must_use]
    pub const fn message_type(&self) -> i64 {
        self.message_type
    }

    /// Message payload bytes.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Whether payload bytes contain PHP serialized wire format.
    #[must_use]
    pub const fn is_serialized(&self) -> bool {
        self.serialized
    }
}

impl SysvMessageQueueState {
    /// Opens or creates a System V queue for a key.
    pub fn get_queue(&mut self, key: i64, permissions: i64) -> i64 {
        if let Some(id) = self.keyed_queues.get(&key).copied()
            && self.queue(id).is_some()
        {
            return id;
        }

        let flags = libc::IPC_CREAT | permissions as libc::c_int;
        let msqid = match sysvmsg_msgget(key as libc::key_t, flags) {
            Ok(msqid) => msqid,
            Err(_) => {
                let id = self.next_id;
                self.next_id += 1;
                self.queues
                    .insert(id, SysvMessageQueue::new(key, -1, permissions));
                self.keyed_queues.insert(key, id);
                return id;
            }
        };
        let id = self.next_id;
        self.next_id += 1;
        self.queues
            .insert(id, SysvMessageQueue::new(key, msqid, permissions));
        self.keyed_queues.insert(key, id);
        id
    }

    /// Binds a PHP-visible queue object handle to its request-local queue id.
    pub fn bind_object(&mut self, object_id: u64, queue_id: i64) {
        self.object_queues.insert(object_id, queue_id);
    }

    /// Looks up the request-local queue id for a PHP-visible queue object.
    #[must_use]
    pub fn queue_id_for_object(&self, object_id: u64) -> Option<i64> {
        self.object_queues.get(&object_id).copied()
    }

    /// Returns whether a live queue exists for the key.
    #[must_use]
    pub fn queue_exists(&self, key: i64) -> bool {
        self.keyed_queues
            .get(&key)
            .is_some_and(|id| self.queue(*id).is_some())
            || sysvmsg_msgget(key as libc::key_t, 0).is_ok()
    }

    /// Returns a live queue.
    #[must_use]
    pub fn queue(&self, id: i64) -> Option<&SysvMessageQueue> {
        self.queues
            .get(&id)
            .filter(|queue| !queue.removed && queue.msqid >= 0 && sysvmsg_stat(queue.msqid).is_ok())
    }

    /// Returns a live queue mutably.
    pub fn queue_mut(&mut self, id: i64) -> Option<&mut SysvMessageQueue> {
        self.queues
            .get_mut(&id)
            .filter(|queue| !queue.removed && queue.msqid >= 0 && sysvmsg_stat(queue.msqid).is_ok())
    }

    /// Removes a queue and keyed lookup.
    pub fn remove_queue(&mut self, id: i64) -> bool {
        let Some(queue) = self.queues.get_mut(&id) else {
            return false;
        };
        if queue.removed {
            return false;
        }
        if queue.msqid < 0 || sysvmsg_msgctl_remove(queue.msqid).is_err() {
            return false;
        }
        queue.removed = true;
        self.keyed_queues.remove(&queue.key);
        true
    }

    /// Enqueues one message.
    pub fn send(&mut self, id: i64, message: SysvMessage, flags: i64) -> Result<(), i32> {
        let Some(queue) = self.queue_mut(id) else {
            return Err(libc::EINVAL);
        };
        sysvmsg_send(
            queue.msqid,
            message.message_type(),
            message.payload(),
            flags,
        )
    }

    /// Enqueues serialized payload bytes while keeping queue internals private.
    pub fn send_payload(
        &mut self,
        id: i64,
        message_type: i64,
        payload: Vec<u8>,
        serialized: bool,
        flags: i64,
    ) -> Result<(), i32> {
        self.send(
            id,
            SysvMessage::new(message_type, payload, serialized),
            flags,
        )
    }

    /// Receives and removes one matching message.
    pub fn receive(
        &mut self,
        id: i64,
        desired_type: i64,
        flags: i64,
        max_size: usize,
    ) -> Result<Option<SysvMessage>, i32> {
        let Some(queue) = self.queue_mut(id) else {
            return Err(libc::EINVAL);
        };
        match sysvmsg_receive(queue.msqid, desired_type, flags, max_size) {
            Ok(message) => Ok(Some(message)),
            Err(error) if error == libc::ENOMSG => Ok(None),
            Err(error) => Err(error),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SysvMessageQueueStats {
    owner_uid: i64,
    owner_gid: i64,
    permissions: i64,
    message_count: usize,
    byte_count: usize,
    max_bytes: i64,
}

#[allow(unsafe_code)] // direct SysV message queue call, result checked
fn sysvmsg_msgget(key: libc::key_t, flags: libc::c_int) -> Result<libc::c_int, i32> {
    // SAFETY: direct SysV IPC call; return value is checked for `-1`.
    let msqid = unsafe { sysvmsg_ffi::msgget(key, flags) };
    if msqid == -1 {
        Err(std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(libc::EIO))
    } else {
        Ok(msqid)
    }
}

fn sysvmsg_stat(msqid: libc::c_int) -> Result<SysvMessageQueueStats, i32> {
    sysvmsg_ffi::stat(msqid)
}

fn sysvmsg_apply_settings(
    msqid: libc::c_int,
    permissions: Option<i64>,
    owner_uid: Option<i64>,
    owner_gid: Option<i64>,
    max_bytes: Option<i64>,
) -> Result<SysvMessageQueueStats, i32> {
    sysvmsg_ffi::apply_settings(msqid, permissions, owner_uid, owner_gid, max_bytes)
}

#[cfg(not(any(target_os = "macos", target_os = "ios")))]
mod sysvmsg_ffi {
    #![allow(unsafe_code)]

    use super::SysvMessageQueueStats;

    pub(super) unsafe fn msgget(key: libc::key_t, flags: libc::c_int) -> libc::c_int {
        // SAFETY: the caller handles the raw return value.
        unsafe { libc::msgget(key, flags) }
    }

    pub(super) unsafe fn msgsnd(
        msqid: libc::c_int,
        msgp: *const libc::c_void,
        msgsz: libc::size_t,
        msgflg: libc::c_int,
    ) -> libc::c_int {
        // SAFETY: the caller provides a valid System V message buffer.
        unsafe { libc::msgsnd(msqid, msgp, msgsz, msgflg) }
    }

    pub(super) unsafe fn msgrcv(
        msqid: libc::c_int,
        msgp: *mut libc::c_void,
        msgsz: libc::size_t,
        msgtyp: libc::c_long,
        msgflg: libc::c_int,
    ) -> libc::ssize_t {
        // SAFETY: the caller provides a valid mutable System V message buffer.
        unsafe { libc::msgrcv(msqid, msgp, msgsz, msgtyp, msgflg) }
    }

    pub(super) fn stat(msqid: libc::c_int) -> Result<SysvMessageQueueStats, i32> {
        let mut stats = std::mem::MaybeUninit::<libc::msqid_ds>::zeroed();
        // SAFETY: `stats` points to valid writable storage for `IPC_STAT`.
        let result = unsafe { libc::msgctl(msqid, libc::IPC_STAT, stats.as_mut_ptr()) };
        if result == -1 {
            return Err(std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or(libc::EIO));
        }
        // SAFETY: `msgctl(IPC_STAT)` succeeded, so the kernel initialized stats.
        let stats = unsafe { stats.assume_init() };
        Ok(SysvMessageQueueStats {
            owner_uid: stats.msg_perm.uid as i64,
            owner_gid: stats.msg_perm.gid as i64,
            permissions: stats.msg_perm.mode as i64,
            message_count: stats.msg_qnum as usize,
            byte_count: stats.msg_cbytes as usize,
            max_bytes: stats.msg_qbytes as i64,
        })
    }

    pub(super) fn apply_settings(
        msqid: libc::c_int,
        permissions: Option<i64>,
        owner_uid: Option<i64>,
        owner_gid: Option<i64>,
        max_bytes: Option<i64>,
    ) -> Result<SysvMessageQueueStats, i32> {
        let mut stats = std::mem::MaybeUninit::<libc::msqid_ds>::zeroed();
        // SAFETY: `stats` points to valid writable storage for `IPC_STAT`.
        let result = unsafe { libc::msgctl(msqid, libc::IPC_STAT, stats.as_mut_ptr()) };
        if result == -1 {
            return Err(std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or(libc::EIO));
        }
        // SAFETY: `msgctl(IPC_STAT)` succeeded, so the kernel initialized stats.
        let mut stats = unsafe { stats.assume_init() };
        if let Some(value) = permissions {
            stats.msg_perm.mode = value as _;
        }
        if let Some(value) = owner_uid {
            stats.msg_perm.uid = value as _;
        }
        if let Some(value) = owner_gid {
            stats.msg_perm.gid = value as _;
        }
        if let Some(value) = max_bytes {
            stats.msg_qbytes = value.max(0) as _;
        }
        // SAFETY: `stats` points to initialized queue metadata for `IPC_SET`.
        let result = unsafe { libc::msgctl(msqid, libc::IPC_SET, &mut stats) };
        if result == -1 {
            Err(std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or(libc::EIO))
        } else {
            stat(msqid)
        }
    }

    pub(super) fn remove(msqid: libc::c_int) -> Result<(), i32> {
        // SAFETY: direct SysV IPC call; return value is checked for `-1`.
        let result = unsafe { libc::msgctl(msqid, libc::IPC_RMID, std::ptr::null_mut()) };
        if result == -1 {
            Err(std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or(libc::EIO))
        } else {
            Ok(())
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod sysvmsg_ffi {
    #![allow(unsafe_code)]

    use super::SysvMessageQueueStats;

    #[repr(C, packed(4))]
    struct DarwinIpcPerm {
        uid: libc::uid_t,
        gid: libc::gid_t,
        cuid: libc::uid_t,
        cgid: libc::gid_t,
        mode: libc::mode_t,
        seq: libc::c_ushort,
        key: libc::key_t,
    }

    #[repr(C, packed(4))]
    struct DarwinMsqidDs {
        msg_perm: DarwinIpcPerm,
        msg_first: i32,
        msg_last: i32,
        msg_cbytes: libc::c_ulong,
        msg_qnum: libc::c_ulong,
        msg_qbytes: libc::c_ulong,
        msg_lspid: libc::pid_t,
        msg_lrpid: libc::pid_t,
        msg_stime: libc::time_t,
        msg_pad1: i32,
        msg_rtime: libc::time_t,
        msg_pad2: i32,
        msg_ctime: libc::time_t,
        msg_pad3: i32,
        msg_pad4: [i32; 4],
    }

    unsafe extern "C" {
        pub(super) fn msgget(key: libc::key_t, flags: libc::c_int) -> libc::c_int;
        pub(super) fn msgsnd(
            msqid: libc::c_int,
            msgp: *const libc::c_void,
            msgsz: libc::size_t,
            msgflg: libc::c_int,
        ) -> libc::c_int;
        pub(super) fn msgrcv(
            msqid: libc::c_int,
            msgp: *mut libc::c_void,
            msgsz: libc::size_t,
            msgtyp: libc::c_long,
            msgflg: libc::c_int,
        ) -> libc::ssize_t;
        fn msgctl(msqid: libc::c_int, cmd: libc::c_int, buf: *mut DarwinMsqidDs) -> libc::c_int;
    }

    pub(super) fn stat(msqid: libc::c_int) -> Result<SysvMessageQueueStats, i32> {
        let mut stats = std::mem::MaybeUninit::<DarwinMsqidDs>::zeroed();
        // SAFETY: `stats` points to valid writable storage for `IPC_STAT`.
        let result = unsafe { msgctl(msqid, libc::IPC_STAT, stats.as_mut_ptr()) };
        if result == -1 {
            return Err(std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or(libc::EIO));
        }
        // SAFETY: `msgctl(IPC_STAT)` succeeded, so the kernel initialized stats.
        let stats = unsafe { stats.assume_init() };
        Ok(stats_to_public(&stats))
    }

    pub(super) fn apply_settings(
        msqid: libc::c_int,
        permissions: Option<i64>,
        owner_uid: Option<i64>,
        owner_gid: Option<i64>,
        max_bytes: Option<i64>,
    ) -> Result<SysvMessageQueueStats, i32> {
        let mut stats = std::mem::MaybeUninit::<DarwinMsqidDs>::zeroed();
        // SAFETY: `stats` points to valid writable storage for `IPC_STAT`.
        let result = unsafe { msgctl(msqid, libc::IPC_STAT, stats.as_mut_ptr()) };
        if result == -1 {
            return Err(std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or(libc::EIO));
        }
        // SAFETY: `msgctl(IPC_STAT)` succeeded, so the kernel initialized stats.
        let mut stats = unsafe { stats.assume_init() };
        if let Some(value) = permissions {
            stats.msg_perm.mode = value as _;
        }
        if let Some(value) = owner_uid {
            stats.msg_perm.uid = value as _;
        }
        if let Some(value) = owner_gid {
            stats.msg_perm.gid = value as _;
        }
        if let Some(value) = max_bytes {
            stats.msg_qbytes = value.max(0) as _;
        }
        // SAFETY: `stats` points to initialized queue metadata for `IPC_SET`.
        let result = unsafe { msgctl(msqid, libc::IPC_SET, &mut stats) };
        if result == -1 {
            Err(std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or(libc::EIO))
        } else {
            stat(msqid)
        }
    }

    pub(super) fn remove(msqid: libc::c_int) -> Result<(), i32> {
        // SAFETY: direct SysV IPC call; return value is checked for `-1`.
        let result = unsafe { msgctl(msqid, libc::IPC_RMID, std::ptr::null_mut()) };
        if result == -1 {
            Err(std::io::Error::last_os_error()
                .raw_os_error()
                .unwrap_or(libc::EIO))
        } else {
            Ok(())
        }
    }

    fn stats_to_public(stats: &DarwinMsqidDs) -> SysvMessageQueueStats {
        SysvMessageQueueStats {
            owner_uid: stats.msg_perm.uid as i64,
            owner_gid: stats.msg_perm.gid as i64,
            permissions: stats.msg_perm.mode as i64,
            message_count: stats.msg_qnum as usize,
            byte_count: stats.msg_cbytes as usize,
            max_bytes: stats.msg_qbytes as i64,
        }
    }
}

fn sysvmsg_msgctl_remove(msqid: libc::c_int) -> Result<(), i32> {
    sysvmsg_ffi::remove(msqid)
}

#[allow(unsafe_code)] // constructs and submits a System V message buffer
fn sysvmsg_send(
    msqid: libc::c_int,
    message_type: i64,
    payload: &[u8],
    flags: i64,
) -> Result<(), i32> {
    let header_len = std::mem::size_of::<libc::c_long>();
    let word_len = std::mem::size_of::<libc::c_long>();
    let total = header_len.saturating_add(payload.len());
    let words = total.div_ceil(word_len).max(1);
    let mut buffer = vec![0 as libc::c_long; words];
    buffer[0] = message_type as libc::c_long;
    // SAFETY: `buffer` is aligned for `c_long` and large enough for header plus
    // payload by construction.
    unsafe {
        std::ptr::copy_nonoverlapping(
            payload.as_ptr(),
            (buffer.as_mut_ptr() as *mut u8).add(header_len),
            payload.len(),
        );
    }
    // SAFETY: buffer layout matches System V `struct msgbuf`.
    let result = unsafe {
        sysvmsg_ffi::msgsnd(
            msqid,
            buffer.as_ptr().cast(),
            payload.len(),
            flags as libc::c_int,
        )
    };
    if result == -1 {
        Err(std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(libc::EIO))
    } else {
        Ok(())
    }
}

#[allow(unsafe_code)] // receives and reads a System V message buffer
fn sysvmsg_receive(
    msqid: libc::c_int,
    desired_type: i64,
    flags: i64,
    max_size: usize,
) -> Result<SysvMessage, i32> {
    let header_len = std::mem::size_of::<libc::c_long>();
    let word_len = std::mem::size_of::<libc::c_long>();
    let total = header_len.saturating_add(max_size);
    let words = total.div_ceil(word_len).max(1);
    let mut buffer = vec![0 as libc::c_long; words];
    // SAFETY: buffer layout matches System V `struct msgbuf`.
    let read = unsafe {
        sysvmsg_ffi::msgrcv(
            msqid,
            buffer.as_mut_ptr().cast(),
            max_size,
            desired_type as libc::c_long,
            flags as libc::c_int,
        )
    };
    if read == -1 {
        return Err(std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(libc::EIO));
    }
    let read = read as usize;
    // SAFETY: `read` is the payload byte count returned by `msgrcv`, and the
    // buffer was allocated with `max_size` payload capacity.
    let payload = unsafe {
        std::slice::from_raw_parts((buffer.as_ptr() as *const u8).add(header_len), read).to_vec()
    };
    Ok(SysvMessage::new(buffer[0] as i64, payload, true))
}

#[allow(unsafe_code)] // direct libc call, result checked
#[cfg(unix)]
fn current_uid() -> i64 {
    unsafe { libc::getuid() as i64 }
}

#[cfg(not(unix))]
fn current_uid() -> i64 {
    0
}

#[allow(unsafe_code)] // direct libc call, result checked
#[cfg(unix)]
fn current_gid() -> i64 {
    unsafe { libc::getgid() as i64 }
}

#[cfg(not(unix))]
fn current_gid() -> i64 {
    0
}

/// System V semaphore backend with a deterministic fallback on non-Unix hosts.
#[derive(Debug, Eq, PartialEq)]
pub struct SysvSemaphoreState {
    next_id: i64,
    semaphores: BTreeMap<i64, SysvSemaphore>,
    keyed_semaphores: BTreeMap<i64, i64>,
}

impl Default for SysvSemaphoreState {
    fn default() -> Self {
        Self {
            next_id: 1,
            semaphores: BTreeMap::new(),
            keyed_semaphores: BTreeMap::new(),
        }
    }
}

/// System V semaphore metadata.
#[derive(Debug, Eq, PartialEq)]
pub struct SysvSemaphore {
    key: i64,
    #[cfg(unix)]
    semid: libc::c_int,
    max_acquire: i64,
    acquired: i64,
    removed: bool,
    auto_release: bool,
}

impl SysvSemaphore {
    #[cfg(unix)]
    fn new(key: i64, semid: libc::c_int, max_acquire: i64, auto_release: bool) -> Self {
        Self {
            key,
            semid,
            max_acquire: max_acquire.max(1),
            acquired: 0,
            removed: false,
            auto_release,
        }
    }

    #[cfg(not(unix))]
    fn new(key: i64, max_acquire: i64, auto_release: bool) -> Self {
        Self {
            key,
            max_acquire: max_acquire.max(1),
            acquired: 0,
            removed: false,
            auto_release,
        }
    }
}

/// SysV semaphore operation result that maps to PHP warnings/false returns.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SysvSemaphoreError {
    Warning(String),
    WouldBlock,
}

impl SysvSemaphoreState {
    /// Opens or creates a semaphore for a key.
    pub fn get(
        &mut self,
        key: i64,
        max_acquire: i64,
        permissions: i64,
        auto_release: bool,
    ) -> Result<i64, SysvSemaphoreError> {
        if let Some(id) = self.keyed_semaphores.get(&key).copied()
            && self.semaphore(id).is_some_and(SysvSemaphore::exists)
        {
            return Ok(id);
        }
        #[cfg(unix)]
        let semaphore = SysvSemaphore::open(key, max_acquire, permissions, auto_release)?;
        #[cfg(not(unix))]
        let semaphore = SysvSemaphore::new(key, max_acquire, auto_release);

        let id = self.next_id;
        self.next_id += 1;
        self.semaphores.insert(id, semaphore);
        self.keyed_semaphores.insert(key, id);
        Ok(id)
    }

    /// Returns a live semaphore.
    #[must_use]
    pub fn semaphore(&self, id: i64) -> Option<&SysvSemaphore> {
        self.semaphores
            .get(&id)
            .filter(|semaphore| !semaphore.removed)
    }

    /// Attempts to acquire a semaphore.
    pub fn acquire(&mut self, id: i64, non_blocking: bool) -> Result<bool, SysvSemaphoreError> {
        let Some(semaphore) = self
            .semaphores
            .get_mut(&id)
            .filter(|semaphore| !semaphore.removed)
        else {
            return Ok(false);
        };
        #[cfg(unix)]
        {
            semaphore.acquire(non_blocking)
        }
        #[cfg(not(unix))]
        {
            let _ = non_blocking;
            if semaphore.acquired >= semaphore.max_acquire {
                return Ok(false);
            }
            semaphore.acquired += 1;
            Ok(true)
        }
    }

    /// Releases one semaphore acquisition.
    pub fn release(&mut self, id: i64) -> Result<bool, SysvSemaphoreError> {
        let Some(semaphore) = self
            .semaphores
            .get_mut(&id)
            .filter(|semaphore| !semaphore.removed)
        else {
            return Ok(false);
        };
        #[cfg(unix)]
        {
            semaphore.release()
        }
        #[cfg(not(unix))]
        {
            if semaphore.acquired <= 0 {
                return Err(SysvSemaphoreError::Warning(format!(
                    "SysV semaphore for key 0x{:x} is not currently acquired",
                    semaphore.key
                )));
            }
            semaphore.acquired -= 1;
            Ok(true)
        }
    }

    /// Removes a semaphore.
    pub fn remove(&mut self, id: i64) -> Result<bool, SysvSemaphoreError> {
        let Some(semaphore) = self.semaphores.get_mut(&id) else {
            return Ok(false);
        };
        if semaphore.removed {
            return Ok(false);
        }
        #[cfg(unix)]
        semaphore.remove()?;
        semaphore.removed = true;
        self.keyed_semaphores.remove(&semaphore.key);
        Ok(true)
    }
}

impl SysvSemaphore {
    #[must_use]
    fn exists(&self) -> bool {
        #[cfg(unix)]
        {
            if self.removed {
                return false;
            }
            sysvsem_ipc_stat(self.semid).is_ok()
        }
        #[cfg(not(unix))]
        {
            !self.removed
        }
    }

    #[allow(unsafe_code)] // direct libc call, result checked
    #[cfg(unix)]
    #[allow(unsafe_code)] // direct SysV semaphore syscall, errno checked
    fn open(
        key: i64,
        max_acquire: i64,
        permissions: i64,
        auto_release: bool,
    ) -> Result<Self, SysvSemaphoreError> {
        const SYSVSEM_SEM: libc::c_ushort = 0;
        const SYSVSEM_USAGE: libc::c_ushort = 1;
        const SYSVSEM_SETVAL: libc::c_ushort = 2;

        let flags = (permissions as libc::c_int) | libc::IPC_CREAT;
        let semid = unsafe { libc::semget(key as libc::key_t, 3, flags) };
        if semid == -1 {
            return Err(SysvSemaphoreError::Warning(format!(
                "Failed for key 0x{key:x}: {}",
                sysvsem_errno_message(sysvsem_errno())
            )));
        }

        let mut lock_ops = [
            sysvsem_op(SYSVSEM_SETVAL, 0, 0),
            sysvsem_op(SYSVSEM_SETVAL, 1, libc::SEM_UNDO),
            sysvsem_op(SYSVSEM_USAGE, 1, libc::SEM_UNDO),
        ];
        if let Err(error) = sysvsem_semop_retry(semid, &mut lock_ops) {
            return Err(SysvSemaphoreError::Warning(format!(
                "Failed acquiring SYSVSEM_SETVAL for key 0x{key:x}: {}",
                sysvsem_errno_message(error)
            )));
        }

        let usage = sysvsem_semctl_getval(semid, SYSVSEM_USAGE).map_err(|error| {
            SysvSemaphoreError::Warning(format!(
                "Failed for key 0x{key:x}: {}",
                sysvsem_errno_message(error)
            ))
        })?;

        if usage == 1 {
            sysvsem_semctl_setval(semid, SYSVSEM_SEM, max_acquire.max(1) as libc::c_int).map_err(
                |error| {
                    SysvSemaphoreError::Warning(format!(
                        "Failed for key 0x{key:x}: {}",
                        sysvsem_errno_message(error)
                    ))
                },
            )?;
        }

        let mut unlock_ops = [sysvsem_op(SYSVSEM_SETVAL, -1, libc::SEM_UNDO)];
        if let Err(error) = sysvsem_semop_retry(semid, &mut unlock_ops) {
            return Err(SysvSemaphoreError::Warning(format!(
                "Failed releasing SYSVSEM_SETVAL for key 0x{key:x}: {}",
                sysvsem_errno_message(error)
            )));
        }

        Ok(Self::new(key, semid, max_acquire, auto_release))
    }

    #[cfg(unix)]
    fn acquire(&mut self, non_blocking: bool) -> Result<bool, SysvSemaphoreError> {
        const SYSVSEM_SEM: libc::c_ushort = 0;
        let flags = libc::SEM_UNDO | if non_blocking { libc::IPC_NOWAIT } else { 0 };
        let mut ops = [sysvsem_op(SYSVSEM_SEM, -1, flags)];
        match sysvsem_semop_retry(self.semid, &mut ops) {
            Ok(()) => {
                self.acquired += 1;
                Ok(true)
            }
            Err(error) if error == libc::EAGAIN => Err(SysvSemaphoreError::WouldBlock),
            Err(error) => Err(SysvSemaphoreError::Warning(format!(
                "Failed to acquire key 0x{:x}: {}",
                self.key,
                sysvsem_errno_message(error)
            ))),
        }
    }

    #[cfg(unix)]
    fn release(&mut self) -> Result<bool, SysvSemaphoreError> {
        const SYSVSEM_SEM: libc::c_ushort = 0;
        if self.acquired <= 0 {
            return Err(SysvSemaphoreError::Warning(format!(
                "SysV semaphore for key 0x{:x} is not currently acquired",
                self.key
            )));
        }
        let mut ops = [sysvsem_op(SYSVSEM_SEM, 1, libc::SEM_UNDO)];
        match sysvsem_semop_retry(self.semid, &mut ops) {
            Ok(()) => {
                self.acquired -= 1;
                Ok(true)
            }
            Err(error) => Err(SysvSemaphoreError::Warning(format!(
                "Failed to release key 0x{:x}: {}",
                self.key,
                sysvsem_errno_message(error)
            ))),
        }
    }

    #[allow(unsafe_code)] // direct libc call, result checked
    #[cfg(unix)]
    #[allow(unsafe_code)] // direct SysV semaphore syscall, errno checked
    fn remove(&mut self) -> Result<(), SysvSemaphoreError> {
        if let Err(error) = sysvsem_ipc_stat(self.semid) {
            return Err(SysvSemaphoreError::Warning(format!(
                "SysV semaphore for key 0x{:x} does not (any longer) exist: {}",
                self.key,
                sysvsem_errno_message(error)
            )));
        }
        if unsafe { libc::semctl(self.semid, 0, libc::IPC_RMID, 0) } == -1 {
            return Err(SysvSemaphoreError::Warning(format!(
                "Failed for SysV semaphore for key 0x{:x}: {}",
                self.key,
                sysvsem_errno_message(sysvsem_errno())
            )));
        }
        self.acquired = -1;
        Ok(())
    }
}

#[cfg(unix)]
fn sysvsem_op(
    sem_num: libc::c_ushort,
    sem_op: libc::c_short,
    sem_flg: libc::c_int,
) -> libc::sembuf {
    libc::sembuf {
        sem_num,
        sem_op,
        sem_flg: sem_flg as libc::c_short,
    }
}

#[allow(unsafe_code)] // direct SysV semaphore syscall, errno checked
#[cfg(unix)]
fn sysvsem_semop_retry(semid: libc::c_int, ops: &mut [libc::sembuf]) -> Result<(), libc::c_int> {
    loop {
        let result = unsafe { libc::semop(semid, ops.as_mut_ptr(), ops.len()) };
        if result == 0 {
            return Ok(());
        }
        let error = sysvsem_errno();
        if error != libc::EINTR {
            return Err(error);
        }
    }
}

#[allow(unsafe_code)] // direct SysV semaphore syscall, errno checked
#[cfg(unix)]
fn sysvsem_semctl_getval(
    semid: libc::c_int,
    sem_num: libc::c_ushort,
) -> Result<libc::c_int, libc::c_int> {
    let value = unsafe { libc::semctl(semid, sem_num as libc::c_int, libc::GETVAL) };
    if value == -1 {
        Err(sysvsem_errno())
    } else {
        Ok(value)
    }
}

#[allow(unsafe_code)] // direct SysV semaphore syscall, errno checked
#[cfg(unix)]
fn sysvsem_semctl_setval(
    semid: libc::c_int,
    sem_num: libc::c_ushort,
    value: libc::c_int,
) -> Result<(), libc::c_int> {
    let result = unsafe { libc::semctl(semid, sem_num as libc::c_int, libc::SETVAL, value) };
    if result == -1 {
        Err(sysvsem_errno())
    } else {
        Ok(())
    }
}

#[allow(unsafe_code)] // direct SysV semaphore syscall, errno checked
#[cfg(unix)]
fn sysvsem_ipc_stat(semid: libc::c_int) -> Result<(), libc::c_int> {
    let mut stat = std::mem::MaybeUninit::<libc::semid_ds>::zeroed();
    let result = unsafe { libc::semctl(semid, 0, libc::IPC_STAT, stat.as_mut_ptr()) };
    if result == -1 {
        Err(sysvsem_errno())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn sysvsem_errno() -> libc::c_int {
    std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(libc::EINVAL)
}

#[cfg(unix)]
fn sysvsem_errno_message(error: libc::c_int) -> String {
    std::io::Error::from_raw_os_error(error).to_string()
}

const SYSVSHM_MAGIC: &[u8; 8] = b"PHRSHM1\0";
const SYSVSHM_HEADER_LEN: usize = SYSVSHM_MAGIC.len() + 4;

/// System V shared variable backend.
#[derive(Debug)]
pub struct SysvSharedMemoryState {
    next_id: i64,
    segments: BTreeMap<i64, SysvSharedMemorySegment>,
    keyed_segments: BTreeMap<i64, i64>,
    object_segments: BTreeMap<u64, i64>,
    destroyed_objects: BTreeSet<u64>,
}

impl Default for SysvSharedMemoryState {
    fn default() -> Self {
        Self {
            next_id: 1,
            segments: BTreeMap::new(),
            keyed_segments: BTreeMap::new(),
            object_segments: BTreeMap::new(),
            destroyed_objects: BTreeSet::new(),
        }
    }
}

/// Host System V shared variable segment.
#[derive(Debug)]
pub struct SysvSharedMemorySegment {
    key: i64,
    shmid: libc::c_int,
    addr: usize,
    size: i64,
    permissions: i64,
    removed: bool,
}

#[allow(unsafe_code)] // direct shared-memory slice views over a live shmat mapping
impl SysvSharedMemorySegment {
    fn new(key: i64, shmid: libc::c_int, addr: usize, size: i64, permissions: i64) -> Self {
        Self {
            key,
            shmid,
            addr,
            size: size.max(0),
            permissions,
            removed: false,
        }
    }

    /// Segment byte capacity.
    #[must_use]
    pub const fn size(&self) -> i64 {
        self.size
    }

    /// Segment permissions.
    #[must_use]
    pub const fn permissions(&self) -> i64 {
        self.permissions
    }

    /// Current stored serialized byte usage.
    #[must_use]
    pub fn byte_count(&self) -> usize {
        self.read_entries().values().map(std::vec::Vec::len).sum()
    }

    /// Returns whether replacing one key with `size` bytes fits in the segment.
    #[must_use]
    pub fn can_store(&self, key: i64, size: usize) -> bool {
        let mut entries = self.read_entries();
        entries.insert(key, vec![0; size]);
        sysvshm_encoded_len(&entries) <= self.size as usize
    }

    /// Stores one serialized shared variable value.
    pub fn put_serialized(&mut self, key: i64, serialized: Vec<u8>) -> bool {
        let mut entries = self.read_entries();
        entries.insert(key, serialized);
        self.write_entries(&entries)
    }

    /// Reads one shared variable value.
    #[must_use]
    pub fn get(&self, key: i64) -> Option<Value> {
        let serialized = self.read_entries().remove(&key)?;
        crate::unserialize(
            &crate::PhpString::from_bytes(serialized),
            crate::UnserializeOptions::default(),
        )
        .ok()
    }

    /// Returns whether a variable key exists.
    #[must_use]
    pub fn has(&self, key: i64) -> bool {
        self.read_entries().contains_key(&key)
    }

    /// Removes one variable key.
    pub fn remove_var(&mut self, key: i64) -> bool {
        let mut entries = self.read_entries();
        if entries.remove(&key).is_none() {
            return false;
        }
        self.write_entries(&entries)
    }

    fn exists(&self) -> bool {
        !self.removed && shmop_segment_size(self.shmid).is_some()
    }

    fn read_entries(&self) -> BTreeMap<i64, Vec<u8>> {
        let bytes = self.data();
        if bytes.len() < SYSVSHM_HEADER_LEN || &bytes[..SYSVSHM_MAGIC.len()] != SYSVSHM_MAGIC {
            return BTreeMap::new();
        }
        let mut offset = SYSVSHM_MAGIC.len();
        let Some(count_bytes) = bytes.get(offset..offset + 4) else {
            return BTreeMap::new();
        };
        let mut count_field = [0; 4];
        count_field.copy_from_slice(count_bytes);
        let count = u32::from_le_bytes(count_field) as usize;
        offset += 4;
        let mut entries = BTreeMap::new();
        for _ in 0..count {
            let Some(key_bytes) = bytes.get(offset..offset + 8) else {
                return BTreeMap::new();
            };
            let mut key_field = [0; 8];
            key_field.copy_from_slice(key_bytes);
            let key = i64::from_le_bytes(key_field);
            offset += 8;
            let Some(len_bytes) = bytes.get(offset..offset + 4) else {
                return BTreeMap::new();
            };
            let mut len_field = [0; 4];
            len_field.copy_from_slice(len_bytes);
            let len = u32::from_le_bytes(len_field) as usize;
            offset += 4;
            let Some(payload) = bytes.get(offset..offset.saturating_add(len)) else {
                return BTreeMap::new();
            };
            offset += len;
            entries.insert(key, payload.to_vec());
        }
        entries
    }

    fn write_entries(&mut self, entries: &BTreeMap<i64, Vec<u8>>) -> bool {
        let encoded_len = sysvshm_encoded_len(entries);
        if encoded_len > self.size as usize {
            return false;
        }
        let data = self.data_mut();
        data.fill(0);
        data[..SYSVSHM_MAGIC.len()].copy_from_slice(SYSVSHM_MAGIC);
        let mut offset = SYSVSHM_MAGIC.len();
        data[offset..offset + 4].copy_from_slice(&(entries.len() as u32).to_le_bytes());
        offset += 4;
        for (key, payload) in entries {
            data[offset..offset + 8].copy_from_slice(&key.to_le_bytes());
            offset += 8;
            data[offset..offset + 4].copy_from_slice(&(payload.len() as u32).to_le_bytes());
            offset += 4;
            data[offset..offset + payload.len()].copy_from_slice(payload);
            offset += payload.len();
        }
        true
    }

    fn data(&self) -> &[u8] {
        if self.addr == 0 || self.size <= 0 {
            return &[];
        }
        // SAFETY: `addr` is a live `shmat` mapping owned by this segment and
        // `size` is the mapped segment size discovered from the kernel.
        unsafe { std::slice::from_raw_parts(self.addr as *const u8, self.size as usize) }
    }

    fn data_mut(&mut self) -> &mut [u8] {
        if self.addr == 0 || self.size <= 0 {
            return &mut [];
        }
        // SAFETY: `addr` is a live writable `shmat` mapping owned by this
        // segment and `size` is the mapped segment size discovered from the kernel.
        unsafe { std::slice::from_raw_parts_mut(self.addr as *mut u8, self.size as usize) }
    }
}

#[allow(unsafe_code)] // direct SysV shared-memory detach during resource drop
impl Drop for SysvSharedMemorySegment {
    fn drop(&mut self) {
        if self.addr != 0 {
            // SAFETY: `addr` was returned by `shmat`; errors during drop cannot
            // be surfaced usefully and do not affect PHP-visible state.
            unsafe {
                libc::shmdt(self.addr as *const libc::c_void);
            }
            self.addr = 0;
        }
    }
}

impl SysvSharedMemoryState {
    /// Attaches to or creates a host System V shared variable segment.
    pub fn attach(&mut self, key: i64, size: i64, permissions: i64) -> Result<i64, i32> {
        if let Some(id) = self.keyed_segments.get(&key).copied()
            && self.segment(id).is_some()
        {
            return Ok(id);
        }
        let shmid = shmop_shmget(
            key as libc::key_t,
            size as usize,
            libc::IPC_CREAT | permissions as libc::c_int,
        )?;
        let mapped_size = shmop_segment_size(shmid).unwrap_or(size as usize) as i64;
        let addr = shmop_attach(shmid, false)?;
        let id = self.next_id;
        self.next_id += 1;
        self.segments.insert(
            id,
            SysvSharedMemorySegment::new(key, shmid, addr, mapped_size, permissions),
        );
        self.keyed_segments.insert(key, id);
        Ok(id)
    }

    /// Binds a PHP-visible shared-memory object handle to a request-local segment.
    pub fn bind_object(&mut self, object_id: u64, segment_id: i64) {
        self.destroyed_objects.remove(&object_id);
        self.object_segments.insert(object_id, segment_id);
    }

    /// Marks a PHP-visible shared-memory object handle as destroyed.
    pub fn destroy_object(&mut self, object_id: u64) {
        self.object_segments.remove(&object_id);
        self.destroyed_objects.insert(object_id);
    }

    /// Returns whether a PHP-visible shared-memory object handle was destroyed.
    #[must_use]
    pub fn object_destroyed(&self, object_id: u64) -> bool {
        self.destroyed_objects.contains(&object_id)
    }

    /// Looks up the request-local segment id for a PHP-visible object handle.
    #[must_use]
    pub fn segment_id_for_object(&self, object_id: u64) -> Option<i64> {
        self.object_segments
            .get(&object_id)
            .copied()
            .filter(|id| self.segment(*id).is_some())
    }

    /// Looks up the request-local segment id for a bound object handle.
    #[must_use]
    pub fn bound_segment_id_for_object(&self, object_id: u64) -> Option<i64> {
        self.object_segments.get(&object_id).copied()
    }

    /// Returns a live segment.
    #[must_use]
    pub fn segment(&self, id: i64) -> Option<&SysvSharedMemorySegment> {
        self.segments.get(&id).filter(|segment| segment.exists())
    }

    /// Returns a live segment mutably.
    pub fn segment_mut(&mut self, id: i64) -> Option<&mut SysvSharedMemorySegment> {
        self.segments
            .get_mut(&id)
            .filter(|segment| segment.exists())
    }

    /// Removes a segment and keyed lookup.
    #[allow(unsafe_code)] // direct SysV shared-memory removal and detach
    pub fn remove(&mut self, id: i64) -> bool {
        let Some(segment) = self.segments.get_mut(&id) else {
            return false;
        };
        if segment.removed {
            return false;
        }
        // SAFETY: direct SysV IPC call; return value is checked for `-1`.
        if unsafe { libc::shmctl(segment.shmid, libc::IPC_RMID, std::ptr::null_mut()) } == -1 {
            return false;
        }
        if segment.addr != 0 {
            // SAFETY: `addr` was returned by `shmat`; detaching here releases
            // the host key immediately after `IPC_RMID` while the PHP handle
            // remains valid for destroyed-handle diagnostics.
            unsafe {
                libc::shmdt(segment.addr as *const libc::c_void);
            }
            segment.addr = 0;
        }
        segment.removed = true;
        self.keyed_segments.remove(&segment.key);
        true
    }
}

fn sysvshm_encoded_len(entries: &BTreeMap<i64, Vec<u8>>) -> usize {
    SYSVSHM_HEADER_LEN
        + entries
            .values()
            .map(|payload| 8usize.saturating_add(4).saturating_add(payload.len()))
            .sum::<usize>()
}

/// Request-local state for `strtok`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StrtokState {
    input: Vec<u8>,
    offset: usize,
    mode: StrtokMode,
    emitted_token: bool,
}

impl StrtokState {
    /// Starts tokenization over a new input string.
    pub fn reset(&mut self, input: Vec<u8>) {
        self.input = input;
        self.offset = 0;
        self.mode = StrtokMode::Active;
        self.emitted_token = false;
    }

    /// Whether one-argument `strtok()` needs a new input string first.
    #[must_use]
    pub const fn requires_input(&self) -> bool {
        matches!(self.mode, StrtokMode::NeedsInput)
    }

    /// Returns the next token separated by any byte in `delimiters`.
    pub fn next_token(&mut self, delimiters: &[u8]) -> Option<Vec<u8>> {
        if delimiters.is_empty() {
            return if self.offset == 0 {
                let token = self.input.clone();
                self.offset = self.input.len();
                Some(token)
            } else {
                None
            };
        }
        let skipped_start = self.offset;
        while self.offset < self.input.len() && delimiters.contains(&self.input[self.offset]) {
            self.offset += 1;
        }
        if self.offset >= self.input.len() {
            // We reached the end while skipping leading delimiters. Because a
            // token's terminating delimiter is now consumed eagerly (see below),
            // reaching the end without skipping any further delimiter is a clean
            // exhaustion; skipping one or more extra trailing delimiters leaves a
            // grace state where the next bare strtok() warns, matching PHP.
            self.mode = if self.input.is_empty()
                || (self.emitted_token && self.offset.saturating_sub(skipped_start) == 0)
            {
                StrtokMode::Exhausted
            } else {
                StrtokMode::NeedsInput
            };
            return None;
        }
        let start = self.offset;
        while self.offset < self.input.len() && !delimiters.contains(&self.input[self.offset]) {
            self.offset += 1;
        }
        let token = self.input[start..self.offset].to_vec();
        // Consume the delimiter that terminated this token so the next call (which
        // may use a different delimiter set) does not re-read it, matching PHP's
        // php_strtok_r, which advances the saved pointer past the delimiter.
        if self.offset < self.input.len() {
            self.offset += 1;
        }
        self.mode = StrtokMode::Active;
        self.emitted_token = true;
        Some(token)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum StrtokMode {
    #[default]
    Exhausted,
    Active,
    NeedsInput,
}

/// Request-local iconv encoding configuration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IconvEncodingState {
    input_encoding: String,
    output_encoding: String,
    internal_encoding: String,
}

impl Default for IconvEncodingState {
    fn default() -> Self {
        Self {
            input_encoding: "UTF-8".to_owned(),
            output_encoding: "UTF-8".to_owned(),
            internal_encoding: "UTF-8".to_owned(),
        }
    }
}

impl IconvEncodingState {
    /// Returns the input encoding used by iconv defaults.
    #[must_use]
    pub fn input_encoding(&self) -> &str {
        &self.input_encoding
    }

    /// Returns the output encoding used by iconv defaults.
    #[must_use]
    pub fn output_encoding(&self) -> &str {
        &self.output_encoding
    }

    /// Returns the internal encoding used by iconv defaults.
    #[must_use]
    pub fn internal_encoding(&self) -> &str {
        &self.internal_encoding
    }

    /// Updates one named iconv encoding setting.
    pub fn set(&mut self, name: &str, encoding: impl Into<String>) -> bool {
        match name {
            "input_encoding" => self.input_encoding = encoding.into(),
            "output_encoding" => self.output_encoding = encoding.into(),
            "internal_encoding" => self.internal_encoding = encoding.into(),
            _ => return false,
        }
        true
    }
}

/// Process-local APCu entry.
#[derive(Clone, Debug, PartialEq)]
struct ApcuEntry {
    value: Value,
    expires_at: Option<SystemTime>,
}

impl ApcuEntry {
    fn is_expired(&self, now: SystemTime) -> bool {
        self.expires_at.is_some_and(|expires_at| expires_at <= now)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
enum ApcuClock {
    #[default]
    System,
    Fixed(SystemTime),
}

impl ApcuClock {
    fn now(&self) -> SystemTime {
        match self {
            Self::System => SystemTime::now(),
            Self::Fixed(now) => *now,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ApcuStore {
    entries: BTreeMap<Vec<u8>, ApcuEntry>,
    hits: u64,
    misses: u64,
    inserts: u64,
    clock: ApcuClock,
}

/// Process-local APCu store handle.
#[derive(Clone, Debug)]
pub struct ApcuState {
    store: Rc<Mutex<ApcuStore>>,
}

impl Default for ApcuState {
    fn default() -> Self {
        process_apcu_state()
    }
}

impl PartialEq for ApcuState {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.store, &other.store)
    }
}

impl ApcuState {
    /// Creates an isolated store for tests or deliberate request-local modes.
    #[must_use]
    pub fn isolated() -> Self {
        Self {
            store: Rc::new(Mutex::new(ApcuStore::default())),
        }
    }

    /// Creates an isolated store with a fixed deterministic clock.
    #[must_use]
    pub fn isolated_at(now: SystemTime) -> Self {
        let store = ApcuStore {
            clock: ApcuClock::Fixed(now),
            ..Default::default()
        };
        Self {
            store: Rc::new(Mutex::new(store)),
        }
    }

    /// Advances the deterministic clock used by isolated test stores.
    pub fn set_test_now(&mut self, now: SystemTime) {
        self.lock_store().clock = ApcuClock::Fixed(now);
    }

    /// Stores a value, replacing any existing key.
    pub fn store(&mut self, key: Vec<u8>, value: Value, ttl: i64) {
        self.lock_store().store(key, value, ttl);
    }

    /// Stores a value only when the key does not already exist.
    pub fn add(&mut self, key: Vec<u8>, value: Value, ttl: i64) -> bool {
        self.lock_store().add(key, value, ttl)
    }

    /// Fetches a value when the key exists and has not expired.
    #[must_use]
    pub fn fetch(&mut self, key: &[u8]) -> Option<Value> {
        self.lock_store().fetch(key)
    }

    /// Returns true when the key exists and has not expired.
    #[must_use]
    pub fn exists(&mut self, key: &[u8]) -> bool {
        self.lock_store().exists(key)
    }

    /// Deletes a key and reports whether it existed.
    pub fn delete(&mut self, key: &[u8]) -> bool {
        self.lock_store().delete(key)
    }

    /// Clears all APCu entries.
    pub fn clear(&mut self) {
        self.lock_store().clear();
    }

    /// Increments an integer value and returns the new value.
    pub fn increment(&mut self, key: &[u8], step: i64) -> Option<i64> {
        self.lock_store().adjust_integer(key, step)
    }

    /// Decrements an integer value and returns the new value.
    pub fn decrement(&mut self, key: &[u8], step: i64) -> Option<i64> {
        self.lock_store().adjust_integer(key, step.checked_neg()?)
    }

    /// Returns a stable statistics snapshot for PHP-visible info functions.
    #[must_use]
    pub fn stats(&mut self) -> ApcuStats {
        self.lock_store().stats()
    }

    fn lock_store(&self) -> MutexGuard<'_, ApcuStore> {
        self.store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl ApcuStore {
    fn store(&mut self, key: Vec<u8>, value: Value, ttl: i64) {
        let expires_at = self.ttl_expiration(ttl);
        self.entries.insert(key, ApcuEntry { value, expires_at });
        self.inserts += 1;
    }

    fn add(&mut self, key: Vec<u8>, value: Value, ttl: i64) -> bool {
        self.purge_expired();
        if self.entries.contains_key(&key) {
            return false;
        }
        self.store(key, value, ttl);
        true
    }

    fn fetch(&mut self, key: &[u8]) -> Option<Value> {
        self.purge_expired();
        match self.entries.get(key) {
            Some(entry) => {
                self.hits += 1;
                Some(entry.value.clone())
            }
            None => {
                self.misses += 1;
                None
            }
        }
    }

    fn exists(&mut self, key: &[u8]) -> bool {
        self.fetch(key).is_some()
    }

    fn delete(&mut self, key: &[u8]) -> bool {
        self.purge_expired();
        self.entries.remove(key).is_some()
    }

    fn clear(&mut self) {
        self.entries.clear();
    }

    fn stats(&mut self) -> ApcuStats {
        self.purge_expired();
        ApcuStats {
            entries: self.entries.len() as u64,
            hits: self.hits,
            misses: self.misses,
            inserts: self.inserts,
        }
    }

    fn adjust_integer(&mut self, key: &[u8], delta: i64) -> Option<i64> {
        self.purge_expired();
        let entry = self.entries.get_mut(key)?;
        let Value::Int(current) = entry.value else {
            self.misses += 1;
            return None;
        };
        let next = current.checked_add(delta)?;
        entry.value = Value::Int(next);
        self.hits += 1;
        Some(next)
    }

    fn purge_expired(&mut self) {
        let now = self.clock.now();
        self.entries.retain(|_, entry| !entry.is_expired(now));
    }

    fn ttl_expiration(&self, ttl: i64) -> Option<SystemTime> {
        if ttl <= 0 {
            None
        } else {
            self.clock
                .now()
                .checked_add(Duration::from_secs(ttl as u64))
        }
    }
}

thread_local! {
    static PROCESS_APCU_STATE: ApcuState = ApcuState::isolated();
}

fn process_apcu_state() -> ApcuState {
    PROCESS_APCU_STATE.with(Clone::clone)
}

/// Stable APCu statistics snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApcuStats {
    pub entries: u64,
    pub hits: u64,
    pub misses: u64,
    pub inserts: u64,
}

/// Request-local OpenSSL error queue.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OpenSslErrorState {
    queue: Vec<String>,
}

impl OpenSslErrorState {
    /// Appends a PHP-visible OpenSSL error string to the request queue.
    pub fn push(&mut self, error: impl Into<String>) {
        self.queue.push(error.into());
    }

    /// Returns and removes the oldest queued OpenSSL error string.
    pub fn pop(&mut self) -> Option<String> {
        if self.queue.is_empty() {
            None
        } else {
            Some(self.queue.remove(0))
        }
    }
}

/// Request-local filesystem process state exposed through standard builtins.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemRuntimeState {
    umask: i64,
}

impl Default for FilesystemRuntimeState {
    fn default() -> Self {
        Self { umask: 0o022 }
    }
}

impl FilesystemRuntimeState {
    /// Returns the current request-local umask.
    #[must_use]
    pub const fn umask(&self) -> i64 {
        self.umask
    }

    /// Updates the request-local umask and returns the previous value.
    pub fn set_umask(&mut self, umask: i64) -> i64 {
        let previous = self.umask;
        self.umask = umask & 0o777;
        previous
    }
}

/// Request-local default stream context options.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StreamContextState {
    default_options: PhpArray,
}

impl StreamContextState {
    /// Returns a snapshot of the current default stream context options.
    #[must_use]
    pub fn default_options(&self) -> PhpArray {
        self.default_options.clone()
    }

    /// Replaces default stream context options.
    pub fn set_default_options(&mut self, options: PhpArray) {
        self.default_options = options;
    }
}

/// Source location passed to internal builtins.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RuntimeSourceSpan {
    /// Optional source file path.
    pub file: Option<String>,
    /// Start byte offset.
    pub start: u32,
    /// End byte offset.
    pub end: u32,
}

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
    filter_inputs: Arc<BTreeMap<i64, crate::PhpArray>>,
    upload_registry: Option<&'a mut UploadRegistry>,
}

pub(in crate::builtins) struct BuiltinExtensionState<'a> {
    pcre_cache: PcreCache,
    pcre_cache_state: Option<&'a mut PcreCache>,
    preg_last_error: pcre::PcreLastErrorState,
    preg_last_error_state: Option<&'a mut pcre::PcreLastErrorState>,
    json_last_error: i64,
    json_last_error_msg: String,
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
    curl_state: CurlState,
    curl_state_slot: Option<&'a mut CurlState>,
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
            pcre_cache: PcreCache::default(),
            pcre_cache_state: None,
            preg_last_error: pcre::PcreLastErrorState::default(),
            preg_last_error_state: None,
            json_last_error: JSON_ERROR_NONE,
            json_last_error_msg: json_error_message(JSON_ERROR_NONE).to_string(),
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
            curl_state: CurlState::default(),
            curl_state_slot: None,
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

/// Mutable runtime services available to internal builtins.
pub struct BuiltinContext<'a> {
    io: BuiltinIoContext<'a>,
    filesystem: BuiltinFilesystemContext<'a>,
    http: BuiltinHttpContext<'a>,
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
        let message = message.into();
        let diagnostic = RuntimeDiagnostic::new(
            id,
            RuntimeSeverity::Warning,
            message,
            source_span,
            Vec::new(),
            Some(crate::PhpReferenceClassification::Warning),
        );
        emit_php_diagnostic(
            self.io.output,
            &diagnostic,
            PhpDiagnosticChannel::Warning,
            PHP_E_WARNING,
            self.io.diagnostic_display,
        );
        self.io.diagnostics.push(diagnostic);
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
        Arc::make_mut(&mut self.http.filter_inputs).insert(source, array);
    }

    /// Shares request-input arrays materialized once during request setup.
    pub fn set_filter_input_arrays_shared(&mut self, arrays: Arc<BTreeMap<i64, crate::PhpArray>>) {
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

    /// Uses VM-owned cURL state for request-local cURL builtins.
    pub fn set_curl_state(&mut self, state: &'a mut CurlState) {
        self.extensions.curl_state_slot = Some(state);
    }

    /// Returns request-local cURL state.
    pub fn curl_state(&mut self) -> &mut CurlState {
        match self.extensions.curl_state_slot.as_deref_mut() {
            Some(state) => state,
            None => &mut self.extensions.curl_state,
        }
    }

    /// Returns immutable request-local cURL state.
    #[must_use]
    pub fn curl_state_ref(&self) -> &CurlState {
        self.extensions
            .curl_state_slot
            .as_deref()
            .unwrap_or(&self.extensions.curl_state)
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

    /// Request-local PCRE pattern cache.
    pub fn pcre_cache(&mut self) -> &mut PcreCache {
        match self.extensions.pcre_cache_state.as_deref_mut() {
            Some(state) => state,
            None => &mut self.extensions.pcre_cache,
        }
    }

    /// Sets request-local PCRE pattern cache storage.
    pub fn set_pcre_cache_state(&mut self, state: &'a mut PcreCache) {
        self.extensions.pcre_cache_state = Some(state);
    }

    /// Sets request-local `preg_last_error` state storage.
    pub fn set_preg_last_error_state(&mut self, state: &'a mut pcre::PcreLastErrorState) {
        self.extensions.preg_last_error_state = Some(state);
    }

    /// Updates request-local PCRE last-error state.
    pub fn set_preg_last_error(&mut self, code: i64, message: impl Into<String>) {
        match self.extensions.preg_last_error_state.as_deref_mut() {
            Some(state) => state.set(code, message),
            None => self.extensions.preg_last_error.set(code, message),
        }
    }

    /// Clears request-local PCRE last-error state.
    pub fn clear_preg_last_error(&mut self) {
        match self.extensions.preg_last_error_state.as_deref_mut() {
            Some(state) => state.clear(),
            None => self.extensions.preg_last_error.clear(),
        }
    }

    /// Returns request-local PCRE last-error code and message.
    #[must_use]
    pub fn preg_last_error(&self) -> (i64, &str) {
        let state = self
            .extensions
            .preg_last_error_state
            .as_deref()
            .unwrap_or(&self.extensions.preg_last_error);
        (state.code(), state.message())
    }

    /// Updates request-local JSON last-error state.
    pub fn set_json_last_error(&mut self, code: i64) {
        self.extensions.json_last_error = code;
        self.extensions.json_last_error_msg = json_error_message(code).to_string();
    }

    /// Returns request-local JSON last-error code and message.
    #[must_use]
    pub fn json_last_error(&self) -> (i64, &str) {
        (
            self.extensions.json_last_error,
            &self.extensions.json_last_error_msg,
        )
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
        BuiltinContext, JSON_ERROR_NONE, JSON_ERROR_SYNTAX, RuntimeSourceSpan, StrtokState,
        json_error_message,
    };
    use crate::{
        ArrayKey, OutputBuffer, PhpArray, PhpString, ReferenceCell, RuntimeHttpResponseState,
        RuntimeUploadedFile, SessionState, UploadRegistry, Value, pcre,
    };
    use std::path::PathBuf;

    #[test]
    fn json_last_error_state_updates_and_reads_from_extension_state() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);

        context.set_json_last_error(JSON_ERROR_SYNTAX);
        assert_eq!(
            context.json_last_error(),
            (JSON_ERROR_SYNTAX, json_error_message(JSON_ERROR_SYNTAX))
        );

        context.set_json_last_error(JSON_ERROR_NONE);
        assert_eq!(
            context.json_last_error(),
            (JSON_ERROR_NONE, json_error_message(JSON_ERROR_NONE))
        );
    }

    #[test]
    fn pcre_last_error_state_updates_and_reads_from_extension_state() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);

        context.set_preg_last_error(2, "backtrack limit exhausted");
        assert_eq!(context.preg_last_error(), (2, "backtrack limit exhausted"));

        context.clear_preg_last_error();
        assert_eq!(
            context.preg_last_error(),
            (
                pcre::PREG_NO_ERROR,
                pcre::preg_error_message(pcre::PREG_NO_ERROR)
            )
        );
    }

    #[test]
    fn pcre_last_error_can_write_through_external_state() {
        let mut output = OutputBuffer::new();
        let mut external = pcre::PcreLastErrorState::default();

        {
            let mut context = BuiltinContext::new(&mut output);
            context.set_preg_last_error_state(&mut external);
            context.set_preg_last_error(3, "recursive limit exhausted");
            assert_eq!(context.preg_last_error(), (3, "recursive limit exhausted"));
        }

        assert_eq!(external.code(), 3);
        assert_eq!(external.message(), "recursive limit exhausted");
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
