//! Runtime services passed to internal builtins.

use crate::{
    FilesystemCapabilities, IniRegistry, MysqlState, OutputBuffer, PHP_E_DEPRECATED, PHP_E_NOTICE,
    PHP_E_WARNING, PcreCache, PhpArray, PhpDiagnosticChannel, PhpDiagnosticDisplayOptions,
    PostgresState, ReferenceCell, ResourceTable, RuntimeDiagnostic, RuntimeHttpResponseState,
    RuntimeSeverity, SessionLoadCallback, SessionState, UploadRegistry, Value, datetime,
    emit_php_diagnostic, pcre,
};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// SysV message queue would-block errno used by the deterministic backend.
pub const SYSVMSG_EAGAIN: i64 = libc::EAGAIN as i64;
pub const SYSVMSG_EINVAL: i64 = libc::EINVAL as i64;

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

/// Request-local state for loopback-gated FTP connections.
#[derive(Debug, Default)]
pub struct FtpState {
    next_id: i64,
    connections: BTreeMap<i64, FtpEntry>,
}

/// Request-local state for the LDAP facade.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LdapState {
    next_connection_id: i64,
    next_result_id: i64,
    next_entry_id: i64,
    connections: BTreeMap<i64, LdapConnectionState>,
    results: BTreeMap<i64, LdapResultState>,
    entries: BTreeMap<i64, LdapResultEntryState>,
    global_options: BTreeMap<i64, Value>,
}

/// Request-local state for the IMAP facade.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImapState {
    next_connection_id: i64,
    connections: BTreeMap<i64, ImapConnectionState>,
    last_errors: Vec<String>,
    last_alerts: Vec<String>,
}

/// Request-local state for the SSH2 facade.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ssh2State {
    next_session_id: i64,
    next_sftp_id: i64,
    sessions: BTreeMap<i64, Ssh2SessionState>,
    sftp_handles: BTreeMap<i64, Ssh2SftpState>,
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

#[derive(Clone, Debug, PartialEq)]
struct ImapConnectionState {
    mailbox: String,
    flags: i64,
    closed: bool,
    deleted_messages: BTreeSet<i64>,
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
            format!(
                "{} for {}:{}",
                session.last_error, session.host, session.port
            )
        })
    }

    /// Creates a request-local SFTP handle attached to a session.
    pub fn sftp(&mut self, session_id: i64) -> Option<i64> {
        if !self.is_open(session_id) {
            return None;
        }
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

    /// Returns whether a session is marked authenticated.
    #[must_use]
    pub fn is_authenticated(&self, id: i64) -> bool {
        self.sessions
            .get(&id)
            .is_some_and(|session| session.authenticated && !session.closed)
    }
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
            Value::Int(0),
        );
        Some(output)
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

#[derive(Debug)]
struct FtpEntry {
    stream: TcpStream,
    passive: bool,
    timeout: Duration,
    auto_seek: bool,
    use_pasv_address: bool,
}

impl FtpState {
    /// Opens a plain FTP control connection and validates the server greeting.
    pub fn connect(&mut self, host: &str, port: u16, timeout_secs: u64) -> Result<i64, i32> {
        let host = loopback_host(host).ok_or(libc::EACCES)?;
        let timeout = Duration::from_secs(timeout_secs.max(1));
        let mut stream = TcpStream::connect((host, port)).map_err(raw_errno)?;
        let _ = stream.set_read_timeout(Some(timeout));
        let _ = stream.set_write_timeout(Some(timeout));
        let response = read_ftp_response(&mut stream).map_err(raw_errno)?;
        if response.code != 220 {
            return Err(libc::ECONNREFUSED);
        }
        let id = if self.next_id <= 0 { 1 } else { self.next_id };
        self.next_id = id.saturating_add(1);
        self.connections.insert(
            id,
            FtpEntry {
                stream,
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
        let response = send_ftp_command(&mut entry.stream, &format!("USER {user}"))?;
        match response.code {
            230 => Ok(true),
            331 => {
                let response = send_ftp_command(&mut entry.stream, &format!("PASS {password}"))?;
                Ok(response.code == 230)
            }
            _ => Ok(false),
        }
    }

    /// Returns the current remote directory from PWD.
    pub fn pwd(&mut self, id: i64) -> Result<Option<String>, i32> {
        let entry = self.connection_mut(id)?;
        let response = send_ftp_command(&mut entry.stream, "PWD")?;
        if response.code != 257 {
            return Ok(None);
        }
        Ok(response
            .lines
            .last()
            .and_then(|line| parse_ftp_quoted_path(line)))
    }

    /// Changes the remote directory with CWD.
    pub fn chdir(&mut self, id: i64, path: &str) -> Result<bool, i32> {
        let entry = self.connection_mut(id)?;
        let response = send_ftp_command(&mut entry.stream, &format!("CWD {path}"))?;
        Ok(response.code == 250)
    }

    /// Changes to the parent directory with CDUP.
    pub fn cdup(&mut self, id: i64) -> Result<bool, i32> {
        let entry = self.connection_mut(id)?;
        let response = send_ftp_command(&mut entry.stream, "CDUP")?;
        Ok(response.code == 200 || response.code == 250)
    }

    /// Runs an EXEC command on servers that support SITE EXEC.
    pub fn exec(&mut self, id: i64, command: &str) -> Result<bool, i32> {
        let entry = self.connection_mut(id)?;
        let response = send_ftp_command(&mut entry.stream, &format!("SITE EXEC {command}"))?;
        Ok((200..300).contains(&response.code))
    }

    /// Sends a raw FTP command and returns response lines without CRLF.
    pub fn raw(&mut self, id: i64, command: &str) -> Result<Vec<String>, i32> {
        let entry = self.connection_mut(id)?;
        Ok(send_ftp_command(&mut entry.stream, command)?.lines)
    }

    /// Creates a remote directory and returns the path reported by the server.
    pub fn mkdir(&mut self, id: i64, path: &str) -> Result<Option<String>, i32> {
        let entry = self.connection_mut(id)?;
        let response = send_ftp_command(&mut entry.stream, &format!("MKD {path}"))?;
        if response.code != 257 && response.code != 250 {
            return Ok(None);
        }
        Ok(response
            .lines
            .last()
            .and_then(|line| parse_ftp_quoted_path(line))
            .or_else(|| Some(path.to_owned())))
    }

    /// Removes a remote directory.
    pub fn rmdir(&mut self, id: i64, path: &str) -> Result<bool, i32> {
        let entry = self.connection_mut(id)?;
        let response = send_ftp_command(&mut entry.stream, &format!("RMD {path}"))?;
        Ok((200..300).contains(&response.code))
    }

    /// Deletes a remote file.
    pub fn delete(&mut self, id: i64, path: &str) -> Result<bool, i32> {
        let entry = self.connection_mut(id)?;
        let response = send_ftp_command(&mut entry.stream, &format!("DELE {path}"))?;
        Ok((200..300).contains(&response.code))
    }

    /// Renames a remote path through RNFR/RNTO.
    pub fn rename(&mut self, id: i64, from: &str, to: &str) -> Result<bool, i32> {
        let entry = self.connection_mut(id)?;
        let response = send_ftp_command(&mut entry.stream, &format!("RNFR {from}"))?;
        if response.code != 350 {
            return Ok(false);
        }
        let response = send_ftp_command(&mut entry.stream, &format!("RNTO {to}"))?;
        Ok((200..300).contains(&response.code))
    }

    /// Sends a SITE command.
    pub fn site(&mut self, id: i64, command: &str) -> Result<bool, i32> {
        let entry = self.connection_mut(id)?;
        let response = send_ftp_command(&mut entry.stream, &format!("SITE {command}"))?;
        Ok((200..300).contains(&response.code))
    }

    /// Sends an ALLO command and returns the server response line.
    pub fn alloc(&mut self, id: i64, size: i64) -> Result<(bool, Option<String>), i32> {
        let entry = self.connection_mut(id)?;
        let response = send_ftp_command(&mut entry.stream, &format!("ALLO {size}"))?;
        Ok((
            response.code == 200 || response.code == 202,
            response.lines.last().cloned(),
        ))
    }

    /// Sends SITE CHMOD and returns the permission on success.
    pub fn chmod(&mut self, id: i64, permissions: i64, path: &str) -> Result<Option<i64>, i32> {
        let entry = self.connection_mut(id)?;
        let response = send_ftp_command(
            &mut entry.stream,
            &format!("SITE CHMOD {permissions:o} {path}"),
        )?;
        Ok(((200..300).contains(&response.code)).then_some(permissions))
    }

    /// Returns the server system type from SYST.
    pub fn systype(&mut self, id: i64) -> Result<Option<String>, i32> {
        let entry = self.connection_mut(id)?;
        let response = send_ftp_command(&mut entry.stream, "SYST")?;
        if response.code != 215 {
            return Ok(None);
        }
        Ok(response
            .lines
            .last()
            .map(|line| line.strip_prefix("215 ").unwrap_or(line).trim().to_owned()))
    }

    /// Returns SIZE, or -1 when the server does not return a numeric 213 value.
    pub fn size(&mut self, id: i64, path: &str) -> Result<i64, i32> {
        let entry = self.connection_mut(id)?;
        let response = send_ftp_command(&mut entry.stream, &format!("SIZE {path}"))?;
        if response.code != 213 {
            return Ok(-1);
        }
        Ok(response
            .lines
            .last()
            .and_then(|line| line.get(4..))
            .and_then(|value| value.trim().parse::<i64>().ok())
            .unwrap_or(-1))
    }

    /// Returns MDTM, or -1 when the server does not return a numeric 213 value.
    pub fn mdtm(&mut self, id: i64, path: &str) -> Result<i64, i32> {
        let entry = self.connection_mut(id)?;
        let response = send_ftp_command(&mut entry.stream, &format!("MDTM {path}"))?;
        if response.code != 213 {
            return Ok(-1);
        }
        Ok(response
            .lines
            .last()
            .and_then(|line| line.get(4..))
            .and_then(|value| value.trim().parse::<i64>().ok())
            .unwrap_or(-1))
    }

    /// Enables or disables passive data-channel mode.
    pub fn set_passive(&mut self, id: i64, enabled: bool) -> Result<bool, i32> {
        let entry = self.connection_mut(id)?;
        entry.passive = enabled;
        Ok(true)
    }

    /// Reads an NLST response through a passive data connection.
    pub fn nlist(&mut self, id: i64, path: &str) -> Result<Option<Vec<String>>, i32> {
        self.read_passive_listing(id, &format!("NLST {path}"))
    }

    /// Reads a LIST response through a passive data connection.
    pub fn rawlist(
        &mut self,
        id: i64,
        path: &str,
        recursive: bool,
    ) -> Result<Option<Vec<String>>, i32> {
        let command = if recursive {
            format!("LIST -R {path}")
        } else {
            format!("LIST {path}")
        };
        self.read_passive_listing(id, &command)
    }

    /// Reads an MLSD response through a passive data connection.
    pub fn mlsd(&mut self, id: i64, path: &str) -> Result<Option<Vec<String>>, i32> {
        self.read_passive_listing(id, &format!("MLSD {path}"))
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
        set_transfer_type(entry, mode)?;
        if offset > 0 {
            let response = send_ftp_command(&mut entry.stream, &format!("REST {offset}"))?;
            if response.code != 350 {
                return Ok(None);
            }
        }
        let mut data = open_passive_data_connection(entry)?;
        let response = send_ftp_command(&mut entry.stream, &format!("RETR {path}"))?;
        if response.code != 125 && response.code != 150 {
            return Ok(None);
        }
        let mut bytes = Vec::new();
        data.read_to_end(&mut bytes).map_err(raw_errno)?;
        drop(data);
        let final_response = read_ftp_response(&mut entry.stream).map_err(raw_errno)?;
        if final_response.code != 226 && final_response.code != 250 {
            return Ok(None);
        }
        Ok(Some(bytes))
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
        set_transfer_type(entry, mode)?;
        if offset > 0 {
            let response = send_ftp_command(&mut entry.stream, &format!("REST {offset}"))?;
            if response.code != 350 {
                return Ok(false);
            }
        }
        let mut data = open_passive_data_connection(entry)?;
        let command = if append { "APPE" } else { "STOR" };
        let response = send_ftp_command(&mut entry.stream, &format!("{command} {path}"))?;
        if response.code != 125 && response.code != 150 {
            return Ok(false);
        }
        data.write_all(bytes).map_err(raw_errno)?;
        data.flush().map_err(raw_errno)?;
        drop(data);
        let final_response = read_ftp_response(&mut entry.stream).map_err(raw_errno)?;
        Ok(final_response.code == 226 || final_response.code == 250)
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
                let _ = entry.stream.set_read_timeout(Some(entry.timeout));
                let _ = entry.stream.set_write_timeout(Some(entry.timeout));
                Ok(true)
            }
            (1, FtpOptionValue::Bool(enabled)) => {
                entry.auto_seek = enabled;
                Ok(true)
            }
            (2, FtpOptionValue::Bool(enabled)) => {
                entry.use_pasv_address = enabled;
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
        let _ = send_ftp_command(&mut entry.stream, "QUIT");
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

    fn read_passive_listing(&mut self, id: i64, command: &str) -> Result<Option<Vec<String>>, i32> {
        let entry = self.connection_mut(id)?;
        if !entry.passive {
            return Ok(None);
        }
        let mut data = open_passive_data_connection(entry)?;
        let response = send_ftp_command(&mut entry.stream, command)?;
        if response.code != 125 && response.code != 150 {
            return Ok(None);
        }
        let mut bytes = Vec::new();
        data.read_to_end(&mut bytes).map_err(raw_errno)?;
        drop(data);
        let final_response = read_ftp_response(&mut entry.stream).map_err(raw_errno)?;
        if final_response.code != 226 && final_response.code != 250 {
            return Ok(None);
        }
        let text = String::from_utf8_lossy(&bytes);
        Ok(Some(
            text.lines()
                .map(|line| line.trim_end_matches('\r').to_owned())
                .filter(|line| !line.is_empty())
                .collect(),
        ))
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
        self.last_error = 0;
        id
    }

    /// Binds a TCP listener to a loopback address.
    pub fn bind_tcp_listener(&mut self, id: i64, address: &str, port: u16) -> Result<(), i32> {
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
        if *domain != i64::from(libc::AF_INET)
            || *socket_type != i64::from(libc::SOCK_STREAM)
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
            Some(_) => Err(libc::EINVAL),
            None => Err(libc::EBADF),
        }
    }

    /// Connects a TCP stream socket to a loopback listener.
    pub fn connect_tcp(&mut self, id: i64, address: &str, port: u16) -> Result<(), i32> {
        let Some(entry) = self.sockets.get_mut(&id) else {
            return Err(libc::EBADF);
        };
        if !matches!(entry, SocketEntry::Created { .. }) {
            return Err(libc::EINVAL);
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
        let listener = match self.sockets.get(&id) {
            Some(SocketEntry::Listener(listener)) => listener,
            Some(_) => return Err(libc::EINVAL),
            None => return Err(libc::EBADF),
        };
        match listener.accept() {
            Ok((stream, _addr)) => {
                let id = if self.next_id <= 0 { 1 } else { self.next_id };
                self.next_id = id.saturating_add(1);
                self.sockets.insert(id, SocketEntry::Stream(stream));
                self.last_error = 0;
                Ok(id)
            }
            Err(error) => Err(raw_errno(error)),
        }
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
            Some(_) => Err(libc::EINVAL),
            None => Err(libc::EBADF),
        }
    }

    /// Returns the local address for a bound or connected socket.
    #[must_use]
    pub fn local_addr(&self, id: i64) -> Option<SocketAddr> {
        match self.sockets.get(&id)? {
            SocketEntry::Listener(listener) => listener.local_addr().ok(),
            SocketEntry::Stream(stream) => stream.local_addr().ok(),
            SocketEntry::Created { .. } | SocketEntry::Closed => None,
        }
    }

    /// Returns the peer address for a connected stream socket.
    #[must_use]
    pub fn peer_addr(&self, id: i64) -> Option<SocketAddr> {
        match self.sockets.get(&id)? {
            SocketEntry::Stream(stream) => stream.peer_addr().ok(),
            SocketEntry::Created { .. } | SocketEntry::Listener(_) | SocketEntry::Closed => None,
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
            Some(_) => Err(libc::EINVAL),
            None => Err(libc::EBADF),
        }
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

fn raw_errno(error: std::io::Error) -> i32 {
    error.raw_os_error().unwrap_or(libc::EIO)
}

#[derive(Debug)]
struct FtpResponse {
    code: i32,
    lines: Vec<String>,
}

fn loopback_host(host: &str) -> Option<&'static str> {
    match host {
        "127.0.0.1" | "localhost" => Some("127.0.0.1"),
        "::1" => Some("::1"),
        _ => None,
    }
}

fn send_ftp_command(stream: &mut TcpStream, command: &str) -> Result<FtpResponse, i32> {
    stream.write_all(command.as_bytes()).map_err(raw_errno)?;
    stream.write_all(b"\r\n").map_err(raw_errno)?;
    stream.flush().map_err(raw_errno)?;
    read_ftp_response(stream).map_err(raw_errno)
}

fn read_ftp_response(stream: &mut TcpStream) -> std::io::Result<FtpResponse> {
    let first = read_ftp_line(stream)?;
    let code = parse_ftp_code(&first).unwrap_or(0);
    let mut lines = vec![first.clone()];
    if first.as_bytes().get(3) == Some(&b'-') {
        let terminator = format!("{code:03} ");
        loop {
            let line = read_ftp_line(stream)?;
            let done = line.starts_with(&terminator);
            lines.push(line);
            if done {
                break;
            }
        }
    }
    Ok(FtpResponse { code, lines })
}

fn read_ftp_line(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut bytes = Vec::new();
    let mut byte = [0_u8; 1];
    loop {
        stream.read_exact(&mut byte)?;
        bytes.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
    }
    if bytes.ends_with(b"\r\n") {
        bytes.truncate(bytes.len().saturating_sub(2));
    } else if bytes.ends_with(b"\n") {
        bytes.truncate(bytes.len().saturating_sub(1));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn parse_ftp_code(line: &str) -> Option<i32> {
    line.get(..3)?.parse().ok()
}

fn open_passive_data_connection(entry: &mut FtpEntry) -> Result<TcpStream, i32> {
    let response = send_ftp_command(&mut entry.stream, "PASV")?;
    if response.code != 227 {
        return Err(libc::ECONNREFUSED);
    }
    let endpoint = response
        .lines
        .last()
        .and_then(|line| parse_pasv_endpoint(line))
        .ok_or(libc::EINVAL)?;
    if !is_loopback_address(&endpoint.host) {
        return Err(libc::EACCES);
    }
    let stream = TcpStream::connect((endpoint.host.as_str(), endpoint.port)).map_err(raw_errno)?;
    let _ = stream.set_read_timeout(Some(entry.timeout));
    let _ = stream.set_write_timeout(Some(entry.timeout));
    Ok(stream)
}

fn set_transfer_type(entry: &mut FtpEntry, mode: i64) -> Result<(), i32> {
    let code = match mode {
        1 => "A",
        2 => "I",
        _ => return Err(libc::EINVAL),
    };
    let response = send_ftp_command(&mut entry.stream, &format!("TYPE {code}"))?;
    if response.code == 200 {
        Ok(())
    } else {
        Err(libc::EIO)
    }
}

#[derive(Debug)]
struct PassiveEndpoint {
    host: String,
    port: u16,
}

fn parse_pasv_endpoint(line: &str) -> Option<PassiveEndpoint> {
    let start = line.find('(')?;
    let end = line[start + 1..].find(')')? + start + 1;
    let values = line[start + 1..end]
        .split(',')
        .map(str::trim)
        .map(str::parse::<u16>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if values.len() != 6 || values[..4].iter().any(|value| *value > 255) {
        return None;
    }
    let host = format!("{}.{}.{}.{}", values[0], values[1], values[2], values[3]);
    let port = values[4].checked_mul(256)?.checked_add(values[5])?;
    Some(PassiveEndpoint { host, port })
}

fn is_loopback_address(address: &str) -> bool {
    address == "localhost" || address == "::1" || address.starts_with("127.")
}

fn parse_ftp_quoted_path(line: &str) -> Option<String> {
    let start = line.find('"')?;
    let mut output = String::new();
    let mut chars = line[start + 1..].chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '"' {
            if chars.peek() == Some(&'"') {
                let _ = chars.next();
                output.push('"');
            } else {
                return Some(output);
            }
        } else {
            output.push(ch);
        }
    }
    None
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

/// Request-local deterministic backend for `shmop`.
#[derive(Clone, Debug, Eq, PartialEq)]
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShmopSegment {
    key: Option<i64>,
    data: Vec<u8>,
    deleted: bool,
}

impl ShmopSegment {
    fn new(key: Option<i64>, size: usize) -> Self {
        Self {
            key,
            data: vec![0; size],
            deleted: false,
        }
    }

    /// Segment byte length.
    #[must_use]
    pub fn size(&self) -> usize {
        self.data.len()
    }

    /// Reads a binary-safe range from the segment.
    #[must_use]
    pub fn read(&self, offset: usize, size: usize) -> Vec<u8> {
        let end = offset.saturating_add(size).min(self.data.len());
        self.data[offset..end].to_vec()
    }

    /// Writes bytes into the segment and returns the count written.
    pub fn write(&mut self, offset: usize, data: &[u8]) -> usize {
        let end = offset.saturating_add(data.len()).min(self.data.len());
        let written = end.saturating_sub(offset);
        self.data[offset..end].copy_from_slice(&data[..written]);
        written
    }

    /// Marks the segment deleted while existing handles may still read it.
    pub fn delete(&mut self) {
        self.deleted = true;
    }

    /// Whether this segment has been deleted.
    #[must_use]
    pub const fn is_deleted(&self) -> bool {
        self.deleted
    }
}

impl ShmopState {
    /// Opens or creates a segment. Key `0` creates private segments.
    pub fn open(&mut self, key: i64, mode: char, size: usize) -> Option<i64> {
        let keyed_id = (key != 0)
            .then(|| self.keyed_segments.get(&key).copied())
            .flatten();
        match mode {
            'a' | 'w' => keyed_id.filter(|id| self.segment(*id).is_some()),
            'c' => keyed_id
                .filter(|id| self.segment(*id).is_some())
                .or_else(|| Some(self.create_segment((key != 0).then_some(key), size))),
            'n' => {
                if keyed_id.is_some_and(|id| self.segment(id).is_some()) {
                    None
                } else {
                    Some(self.create_segment((key != 0).then_some(key), size))
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

    fn create_segment(&mut self, key: Option<i64>, size: usize) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        self.segments.insert(id, ShmopSegment::new(key, size));
        if let Some(key) = key {
            self.keyed_segments.insert(key, id);
        }
        id
    }
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

/// Request-local deterministic backend for System V message queues.
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

/// Request-local message queue metadata and pending messages.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SysvMessageQueue {
    key: i64,
    permissions: i64,
    owner_uid: i64,
    owner_gid: i64,
    messages: Vec<SysvMessage>,
    removed: bool,
    max_bytes: i64,
}

impl SysvMessageQueue {
    fn new(key: i64, permissions: i64) -> Self {
        Self {
            key,
            permissions,
            owner_uid: current_uid(),
            owner_gid: current_gid(),
            messages: Vec::new(),
            removed: false,
            max_bytes: 16_384,
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
        self.messages.len()
    }

    /// Current pending payload byte count.
    #[must_use]
    pub fn byte_count(&self) -> usize {
        self.messages
            .iter()
            .map(|message| message.payload.len())
            .sum()
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
    /// Opens or creates a request-local queue for a key.
    pub fn get_queue(&mut self, key: i64, permissions: i64) -> i64 {
        if let Some(id) = self.keyed_queues.get(&key).copied()
            && self.queue(id).is_some()
        {
            return id;
        }

        let id = self.next_id;
        self.next_id += 1;
        self.queues
            .insert(id, SysvMessageQueue::new(key, permissions));
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
    }

    /// Returns a live queue.
    #[must_use]
    pub fn queue(&self, id: i64) -> Option<&SysvMessageQueue> {
        self.queues.get(&id).filter(|queue| !queue.removed)
    }

    /// Returns a live queue mutably.
    pub fn queue_mut(&mut self, id: i64) -> Option<&mut SysvMessageQueue> {
        self.queues.get_mut(&id).filter(|queue| !queue.removed)
    }

    /// Removes a queue and keyed lookup.
    pub fn remove_queue(&mut self, id: i64) -> bool {
        let Some(queue) = self.queues.get_mut(&id) else {
            return false;
        };
        if queue.removed {
            return false;
        }
        queue.removed = true;
        queue.messages.clear();
        self.keyed_queues.remove(&queue.key);
        true
    }

    /// Enqueues one message.
    pub fn send(&mut self, id: i64, message: SysvMessage) -> bool {
        let Some(queue) = self.queue_mut(id) else {
            return false;
        };
        queue.messages.push(message);
        true
    }

    /// Enqueues serialized payload bytes while keeping queue internals private.
    pub fn send_payload(
        &mut self,
        id: i64,
        message_type: i64,
        payload: Vec<u8>,
        serialized: bool,
    ) -> bool {
        self.send(id, SysvMessage::new(message_type, payload, serialized))
    }

    /// Receives and removes one matching message.
    pub fn receive(&mut self, id: i64, desired_type: i64, except: bool) -> Option<SysvMessage> {
        let queue = self.queue_mut(id)?;
        let index = queue.messages.iter().position(|message| {
            let message_type = message.message_type();
            if except {
                message_type != desired_type
            } else if desired_type == 0 {
                true
            } else if desired_type > 0 {
                message_type == desired_type
            } else {
                message_type <= desired_type.abs()
            }
        })?;
        Some(queue.messages.remove(index))
    }
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

#[allow(unsafe_code)] // direct libc call, result checked
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

#[allow(unsafe_code)] // direct libc call, result checked
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

#[allow(unsafe_code)] // direct libc call, result checked
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

#[allow(unsafe_code)] // direct libc call, result checked
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

/// Request-local deterministic backend for System V shared variables.
#[derive(Clone, Debug, Eq, PartialEq)]
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

/// Request-local shared variable segment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SysvSharedMemorySegment {
    key: i64,
    size: i64,
    permissions: i64,
    values: BTreeMap<i64, Value>,
    value_sizes: BTreeMap<i64, usize>,
    removed: bool,
}

impl SysvSharedMemorySegment {
    fn new(key: i64, size: i64, permissions: i64) -> Self {
        Self {
            key,
            size: size.max(0),
            permissions,
            values: BTreeMap::new(),
            value_sizes: BTreeMap::new(),
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
        self.value_sizes.values().copied().sum()
    }

    /// Returns whether replacing one key with `size` bytes fits in the segment.
    #[must_use]
    pub fn can_store(&self, key: i64, size: usize) -> bool {
        let previous = self.value_sizes.get(&key).copied().unwrap_or(0);
        self.byte_count()
            .saturating_sub(previous)
            .saturating_add(size)
            <= self.size as usize
    }

    /// Stores one shared variable value.
    pub fn put(&mut self, key: i64, value: Value, size: usize) {
        self.value_sizes.insert(key, size);
        self.values.insert(key, value);
    }

    /// Reads one shared variable value.
    #[must_use]
    pub fn get(&self, key: i64) -> Option<Value> {
        self.values.get(&key).cloned()
    }

    /// Returns whether a variable key exists.
    #[must_use]
    pub fn has(&self, key: i64) -> bool {
        self.values.contains_key(&key)
    }

    /// Removes one variable key.
    pub fn remove_var(&mut self, key: i64) -> bool {
        self.value_sizes.remove(&key);
        self.values.remove(&key).is_some()
    }
}

impl SysvSharedMemoryState {
    /// Attaches to or creates a request-local shared variable segment.
    pub fn attach(&mut self, key: i64, size: i64, permissions: i64) -> i64 {
        if let Some(id) = self.keyed_segments.get(&key).copied()
            && self.segment(id).is_some()
        {
            return id;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.segments
            .insert(id, SysvSharedMemorySegment::new(key, size, permissions));
        self.keyed_segments.insert(key, id);
        id
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
        self.segments.get(&id).filter(|segment| !segment.removed)
    }

    /// Returns a live segment mutably.
    pub fn segment_mut(&mut self, id: i64) -> Option<&mut SysvSharedMemorySegment> {
        self.segments
            .get_mut(&id)
            .filter(|segment| !segment.removed)
    }

    /// Removes a segment and keyed lookup.
    pub fn remove(&mut self, id: i64) -> bool {
        let Some(segment) = self.segments.get_mut(&id) else {
            return false;
        };
        if segment.removed {
            return false;
        }
        segment.removed = true;
        segment.values.clear();
        segment.value_sizes.clear();
        self.keyed_segments.remove(&segment.key);
        true
    }
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

/// Request-local APCu entry.
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

/// Request-local APCu store.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ApcuState {
    entries: BTreeMap<Vec<u8>, ApcuEntry>,
    hits: u64,
    misses: u64,
    inserts: u64,
}

impl ApcuState {
    /// Stores a value, replacing any existing key.
    pub fn store(&mut self, key: Vec<u8>, value: Value, ttl: i64) {
        let expires_at = ttl_expiration(ttl);
        self.entries.insert(key, ApcuEntry { value, expires_at });
        self.inserts += 1;
    }

    /// Stores a value only when the key does not already exist.
    pub fn add(&mut self, key: Vec<u8>, value: Value, ttl: i64) -> bool {
        self.purge_expired();
        if self.entries.contains_key(&key) {
            return false;
        }
        self.store(key, value, ttl);
        true
    }

    /// Fetches a value when the key exists and has not expired.
    #[must_use]
    pub fn fetch(&mut self, key: &[u8]) -> Option<Value> {
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

    /// Returns true when the key exists and has not expired.
    #[must_use]
    pub fn exists(&mut self, key: &[u8]) -> bool {
        self.fetch(key).is_some()
    }

    /// Deletes a key and reports whether it existed.
    pub fn delete(&mut self, key: &[u8]) -> bool {
        self.purge_expired();
        self.entries.remove(key).is_some()
    }

    /// Clears all APCu entries.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Increments an integer value and returns the new value.
    pub fn increment(&mut self, key: &[u8], step: i64) -> Option<i64> {
        self.adjust_integer(key, step)
    }

    /// Decrements an integer value and returns the new value.
    pub fn decrement(&mut self, key: &[u8], step: i64) -> Option<i64> {
        self.adjust_integer(key, step.checked_neg()?)
    }

    /// Returns a stable statistics snapshot for PHP-visible info functions.
    #[must_use]
    pub fn stats(&mut self) -> ApcuStats {
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
        let now = SystemTime::now();
        self.entries.retain(|_, entry| !entry.is_expired(now));
    }
}

/// Stable APCu statistics snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApcuStats {
    pub entries: u64,
    pub hits: u64,
    pub misses: u64,
    pub inserts: u64,
}

fn ttl_expiration(ttl: i64) -> Option<SystemTime> {
    if ttl <= 0 {
        None
    } else {
        Some(SystemTime::now() + Duration::from_secs(ttl as u64))
    }
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
    php_input: Vec<u8>,
    diagnostic_display: PhpDiagnosticDisplayOptions,
    diagnostics: Vec<RuntimeDiagnostic>,
}

impl<'a> BuiltinIoContext<'a> {
    fn new(output: &'a mut OutputBuffer) -> Self {
        Self {
            output,
            php_input: Vec::new(),
            diagnostic_display: PhpDiagnosticDisplayOptions::default(),
            diagnostics: Vec::new(),
        }
    }
}

pub(in crate::builtins) struct BuiltinFilesystemContext<'a> {
    cwd: PathBuf,
    include_path: Vec<PathBuf>,
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
            include_path: vec![PathBuf::from(".")],
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
    filter_inputs: BTreeMap<i64, crate::PhpArray>,
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
    mb_substitute_character: MbSubstituteCharacter,
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
            mb_substitute_character: MbSubstituteCharacter::Codepoint(63),
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
    default_timezone: String,
    env: Vec<(String, String)>,
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
            default_timezone: datetime::DEFAULT_TIMEZONE.to_string(),
            env: Vec::new(),
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
            default_timezone: datetime::DEFAULT_TIMEZONE.to_string(),
            env: Vec::new(),
            network_requests_enabled: false,
        }
    }

    /// Sets deterministic request-local environment entries.
    pub fn set_env_entries(&mut self, mut env: Vec<(String, String)>) {
        env.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
        self.env = env;
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
        &self.filesystem.cwd
    }

    /// Updates the request-local current working directory for filesystem builtins.
    pub fn set_cwd(&mut self, cwd: impl Into<PathBuf>) {
        self.filesystem.cwd = cwd.into();
    }

    /// Include path entries used by stream include-path resolution.
    #[must_use]
    pub fn include_path(&self) -> &[PathBuf] {
        &self.filesystem.include_path
    }

    /// Sets request-local include path entries.
    pub fn set_include_path(&mut self, include_path: Vec<PathBuf>) {
        self.filesystem.include_path = include_path;
    }

    /// Reads a request-local INI option visible to standard-library builtins.
    #[must_use]
    pub fn ini_get(&self, name: &str) -> Option<&str> {
        self.ini.get(name)
    }

    /// Sets request-local INI options visible to standard-library builtins.
    pub fn set_ini_registry(&mut self, ini: IniRegistry) {
        self.ini = ini;
    }

    /// Returns request-local INI options visible to standard-library builtins.
    #[must_use]
    pub const fn ini_registry(&self) -> &IniRegistry {
        &self.ini
    }

    /// Updates a request-local INI option visible to standard-library builtins.
    pub fn ini_set(&mut self, name: &str, value: impl Into<String>) -> Option<String> {
        self.ini.set(name, value)
    }

    /// Current request-local default timezone.
    #[must_use]
    pub fn default_timezone(&self) -> &str {
        &self.default_timezone
    }

    /// Updates the request-local default timezone.
    pub fn set_default_timezone(&mut self, identifier: impl Into<String>) {
        self.default_timezone = identifier.into();
    }

    /// Filesystem capabilities for path and filesystem builtins.
    #[must_use]
    pub fn filesystem_capabilities(&self) -> &FilesystemCapabilities {
        &self.filesystem.filesystem
    }

    /// Sets deterministic bytes exposed through `php://input`.
    pub fn set_php_input(&mut self, input: Vec<u8>) {
        self.io.php_input = input;
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
        self.http.filter_inputs.insert(source, array);
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

    /// Sets request-local APCu state.
    pub fn set_apcu_state(&mut self, state: &'a mut ApcuState) {
        self.extensions.apcu_state_slot = Some(state);
    }

    /// Mutable request-local APCu state.
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
        &self.extensions.mb_internal_encoding
    }

    /// Updates the request-local mbstring internal encoding.
    pub fn set_mb_internal_encoding(&mut self, encoding: impl Into<String>) {
        self.extensions.mb_internal_encoding = encoding.into();
    }

    /// Current request-local mbstring substitute-character mode.
    #[must_use]
    pub fn mb_substitute_character(&self) -> &MbSubstituteCharacter {
        &self.extensions.mb_substitute_character
    }

    /// Updates the request-local mbstring substitute-character mode.
    pub fn set_mb_substitute_character(&mut self, substitute: MbSubstituteCharacter) {
        self.extensions.mb_substitute_character = substitute;
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
