//! Capability-gated MySQL/MariaDB client layer.

use crate::{ArrayKey, PhpArray, PhpString, Value, convert};
use mysql::{Conn, Opts, Params, Row, Value as MysqlValue, prelude::Queryable};
use rusqlite::types::{Value as SqliteValue, ValueRef as SqliteValueRef};
use rusqlite::{Connection as SqliteConnection, OpenFlags, params_from_iter};
use std::collections::HashMap;
use std::env;
use std::fmt;

/// Environment variable that enables live MySQL/MariaDB tests.
pub const MYSQL_TEST_DSN_ENV: &str = "PHRUST_MYSQL_TEST_DSN";
/// Environment variable that enables the deterministic mysqli SQLite adapter.
pub const MYSQLI_SQLITE_COMPAT_ENV: &str = "PHRUST_MYSQLI_SQLITE_COMPAT";
/// `mysqlnd`-style client info reported by the mysqli MVP.
pub const MYSQLND_CLIENT_INFO: &str = "mysqlnd 8.5.7";
/// `mysqlnd`-style client version reported by the mysqli MVP.
pub const MYSQLND_CLIENT_VERSION: i64 = 80507;

/// `mysqli_fetch_array()` associative columns.
pub const MYSQLI_ASSOC: i64 = 1;
/// `mysqli_fetch_array()` numeric columns.
pub const MYSQLI_NUM: i64 = 2;
/// `mysqli_fetch_array()` associative and numeric columns.
pub const MYSQLI_BOTH: i64 = MYSQLI_ASSOC | MYSQLI_NUM;
/// `mysqli_report()` silent mode.
pub const MYSQLI_REPORT_OFF: i64 = 0;
/// `mysqli_report()` error-reporting mode.
pub const MYSQLI_REPORT_ERROR: i64 = 1;
/// `mysqli_report()` strict exception-reporting mode.
pub const MYSQLI_REPORT_STRICT: i64 = 2;
/// `mysqli_report()` index-reporting mode.
pub const MYSQLI_REPORT_INDEX: i64 = 4;

#[derive(Clone, Debug, Eq, PartialEq)]
struct MysqlBufferedResult {
    columns: Vec<String>,
    rows: Vec<MysqlRow>,
    offset: usize,
}

#[derive(Debug)]
struct MysqlRuntimeConnection {
    connection: MysqlConnectionBackend,
    last_errno: i64,
    last_error: String,
    last_field_count: i64,
    affected_rows: i64,
    last_insert_id: i64,
}

#[derive(Debug)]
enum MysqlConnectionBackend {
    Live(MysqlConnection),
    SqliteCompat(MysqliSqliteCompatConnection),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MysqlRuntimeStatement {
    connection_id: i64,
    sql: Option<String>,
    last_result_id: Option<i64>,
    last_errno: i64,
    last_error: String,
    sqlstate: String,
    affected_rows: i64,
    last_insert_id: i64,
}

impl MysqlRuntimeStatement {
    fn new(connection_id: i64) -> Self {
        Self {
            connection_id,
            sql: None,
            last_result_id: None,
            last_errno: 0,
            last_error: String::new(),
            sqlstate: "00000".to_owned(),
            affected_rows: 0,
            last_insert_id: 0,
        }
    }
}

/// Request-local MySQL/MariaDB connections and buffered result sets.
#[derive(Default)]
pub struct MysqlState {
    next_connection_id: i64,
    connections: HashMap<i64, MysqlRuntimeConnection>,
    next_result_id: i64,
    results: HashMap<i64, MysqlBufferedResult>,
    next_statement_id: i64,
    statements: HashMap<i64, MysqlRuntimeStatement>,
    connect_errno: i64,
    connect_error: String,
    report_flags: i64,
}

impl fmt::Debug for MysqlState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MysqlState")
            .field("next_connection_id", &self.next_connection_id)
            .field("connections", &self.connections.keys().collect::<Vec<_>>())
            .field("next_result_id", &self.next_result_id)
            .field("results", &self.results.keys().collect::<Vec<_>>())
            .field("next_statement_id", &self.next_statement_id)
            .field("statements", &self.statements.keys().collect::<Vec<_>>())
            .field("connect_errno", &self.connect_errno)
            .field("connect_error", &self.connect_error)
            .field("report_flags", &self.report_flags)
            .finish()
    }
}

impl MysqlState {
    /// Updates request-local `mysqli_report()` policy flags.
    pub fn set_report_flags(&mut self, flags: i64) {
        self.report_flags = flags;
    }

    /// Returns request-local `mysqli_report()` policy flags.
    #[must_use]
    pub const fn report_flags(&self) -> i64 {
        self.report_flags
    }

    /// Records a deterministic connection failure without opening a socket.
    pub fn record_connect_error(&mut self, errno: i64, message: impl Into<String>) {
        self.connect_errno = errno;
        self.connect_error = message.into();
    }

    /// Opens a live MySQL/MariaDB connection from an explicit DSN.
    pub fn connect(&mut self, options: &MysqlConnectOptions) -> Result<i64, MysqlError> {
        match MysqlConnection::connect(options) {
            Ok(connection) => {
                let id = self.insert_connection(MysqlConnectionBackend::Live(connection));
                self.connect_errno = 0;
                self.connect_error.clear();
                Ok(id)
            }
            Err(error) => {
                self.connect_errno = error.mysql_errno();
                self.connect_error = error.message.clone();
                Err(error)
            }
        }
    }

    /// Opens a deterministic request-local SQLite-backed mysqli compatibility
    /// connection. This is not MySQL protocol parity; callers must keep it
    /// behind explicit selected fixtures or application-bootstrap gates.
    pub fn connect_sqlite_compat(&mut self) -> Result<i64, MysqlError> {
        match MysqliSqliteCompatConnection::open_memory() {
            Ok(connection) => {
                let id = self.insert_connection(MysqlConnectionBackend::SqliteCompat(connection));
                self.connect_errno = 0;
                self.connect_error.clear();
                Ok(id)
            }
            Err(error) => {
                self.connect_errno = error.mysql_errno();
                self.connect_error = error.message.clone();
                Err(error)
            }
        }
    }

    /// Closes an open connection.
    pub fn close(&mut self, id: i64) -> bool {
        let removed = self.connections.remove(&id).is_some();
        if removed {
            self.statements
                .retain(|_, statement| statement.connection_id != id);
        }
        removed
    }

    /// Runs a simple text query. Row-returning results are buffered and return a
    /// result id; non-row statements return `None` after successful execution.
    pub fn query(&mut self, id: i64, sql: &str) -> Result<Option<i64>, MysqlError> {
        let Some(connection) = self.connections.get_mut(&id) else {
            return Err(MysqlError::new(
                MysqlErrorKind::Client,
                "not an open MySQL connection",
            ));
        };
        match connection.connection.query(sql) {
            Ok(result) => {
                connection.last_errno = 0;
                connection.last_error.clear();
                connection.affected_rows = result.affected_rows;
                connection.last_insert_id = result.last_insert_id;
                connection.last_field_count = result.columns.len().try_into().unwrap_or(i64::MAX);
                if result.columns.is_empty() {
                    return Ok(None);
                }
                Ok(Some(self.insert_result(result)))
            }
            Err(error) => {
                connection.last_errno = error.mysql_errno();
                connection.last_error = error.message.clone();
                Err(error)
            }
        }
    }

    /// Creates an empty statement handle associated with a connection.
    pub fn stmt_init(&mut self, connection_id: i64) -> Result<i64, MysqlError> {
        if !self.connections.contains_key(&connection_id) {
            return Err(MysqlError::new(
                MysqlErrorKind::Client,
                "not an open MySQL connection",
            ));
        }
        self.next_statement_id = self.next_statement_id.saturating_add(1).max(1);
        let id = self.next_statement_id;
        self.statements
            .insert(id, MysqlRuntimeStatement::new(connection_id));
        Ok(id)
    }

    /// Creates and prepares a statement handle.
    pub fn prepare_statement(&mut self, connection_id: i64, sql: &str) -> Result<i64, MysqlError> {
        let statement_id = self.stmt_init(connection_id)?;
        match self.stmt_prepare(statement_id, sql) {
            Ok(()) => Ok(statement_id),
            Err(error) => {
                self.statements.remove(&statement_id);
                Err(error)
            }
        }
    }

    /// Prepares SQL on an existing statement handle.
    pub fn stmt_prepare(&mut self, statement_id: i64, sql: &str) -> Result<(), MysqlError> {
        if sql.trim().is_empty() {
            let error = MysqlError::new(
                MysqlErrorKind::InvalidQuery,
                "MySQL query must not be empty",
            );
            self.record_stmt_error(statement_id, &error);
            return Err(error);
        }
        let connection_id = self
            .statements
            .get(&statement_id)
            .map(|statement| statement.connection_id)
            .ok_or_else(|| MysqlError::new(MysqlErrorKind::Client, "not an open mysqli_stmt"))?;
        let Some(connection) = self.connections.get_mut(&connection_id) else {
            let error = MysqlError::new(MysqlErrorKind::Client, "not an open MySQL connection");
            self.record_stmt_error(statement_id, &error);
            return Err(error);
        };
        match connection.connection.prepare(sql) {
            Ok(()) => {
                if let Some(statement) = self.statements.get_mut(&statement_id) {
                    statement.sql = Some(sql.to_owned());
                    statement.last_errno = 0;
                    statement.last_error.clear();
                    statement.sqlstate = "00000".to_owned();
                }
                connection.last_errno = 0;
                connection.last_error.clear();
                Ok(())
            }
            Err(error) => {
                connection.last_errno = error.mysql_errno();
                connection.last_error = error.message.clone();
                self.record_stmt_error(statement_id, &error);
                Err(error)
            }
        }
    }

    /// Executes a prepared statement with positional PHP parameter values.
    pub fn stmt_execute(
        &mut self,
        statement_id: i64,
        params: &[Value],
    ) -> Result<bool, MysqlError> {
        let (connection_id, sql) = {
            let statement = self.statements.get(&statement_id).ok_or_else(|| {
                MysqlError::new(MysqlErrorKind::Client, "not an open mysqli_stmt")
            })?;
            let Some(sql) = &statement.sql else {
                let error =
                    MysqlError::new(MysqlErrorKind::InvalidQuery, "mysqli_stmt is not prepared");
                self.record_stmt_error(statement_id, &error);
                return Err(error);
            };
            (statement.connection_id, sql.clone())
        };
        let result = {
            let Some(connection) = self.connections.get_mut(&connection_id) else {
                let error = MysqlError::new(MysqlErrorKind::Client, "not an open MySQL connection");
                self.record_stmt_error(statement_id, &error);
                return Err(error);
            };
            match connection.connection.execute_prepared(&sql, params) {
                Ok(result) => {
                    connection.last_errno = 0;
                    connection.last_error.clear();
                    connection.affected_rows = result.affected_rows;
                    connection.last_insert_id = result.last_insert_id;
                    connection.last_field_count =
                        result.columns.len().try_into().unwrap_or(i64::MAX);
                    Ok(result)
                }
                Err(error) => {
                    connection.last_errno = error.mysql_errno();
                    connection.last_error = error.message.clone();
                    Err(error)
                }
            }
        };
        match result {
            Ok(result) => {
                let affected_rows = result.affected_rows;
                let last_insert_id = result.last_insert_id;
                let result_id = if result.columns.is_empty() {
                    None
                } else {
                    Some(self.insert_result(result))
                };
                if let Some(statement) = self.statements.get_mut(&statement_id) {
                    statement.last_result_id = result_id;
                    statement.last_errno = 0;
                    statement.last_error.clear();
                    statement.sqlstate = "00000".to_owned();
                    statement.affected_rows = affected_rows;
                    statement.last_insert_id = last_insert_id;
                }
                Ok(true)
            }
            Err(error) => {
                self.record_stmt_error(statement_id, &error);
                Err(error)
            }
        }
    }

    /// Returns the buffered result handle from the most recent statement execute.
    #[must_use]
    pub fn stmt_result(&self, statement_id: i64) -> Option<i64> {
        self.statements
            .get(&statement_id)
            .and_then(|statement| statement.last_result_id)
    }

    /// Fetches the next row from a statement result as scalar values.
    pub fn stmt_fetch_row(&mut self, statement_id: i64) -> Option<Vec<Value>> {
        let result_id = self.stmt_result(statement_id)?;
        let result = self.results.get_mut(&result_id)?;
        let row = result.rows.get(result.offset).cloned()?;
        result.offset = result.offset.saturating_add(1);
        Some(row.values.iter().map(cell_to_value).collect())
    }

    /// Returns one scalar property of a statement handle.
    #[must_use]
    pub fn stmt_num_rows(&self, statement_id: i64) -> i64 {
        self.stmt_result(statement_id)
            .map_or(0, |result_id| self.num_rows(result_id))
    }

    #[must_use]
    pub fn stmt_affected_rows(&self, statement_id: i64) -> i64 {
        self.statements
            .get(&statement_id)
            .map_or(-1, |statement| statement.affected_rows)
    }

    #[must_use]
    pub fn stmt_insert_id(&self, statement_id: i64) -> i64 {
        self.statements
            .get(&statement_id)
            .map_or(0, |statement| statement.last_insert_id)
    }

    #[must_use]
    pub fn stmt_errno(&self, statement_id: i64) -> i64 {
        self.statements
            .get(&statement_id)
            .map_or(1, |statement| statement.last_errno)
    }

    #[must_use]
    pub fn stmt_error(&self, statement_id: i64) -> String {
        self.statements.get(&statement_id).map_or_else(
            || "not an open mysqli_stmt".to_owned(),
            |statement| statement.last_error.clone(),
        )
    }

    #[must_use]
    pub fn stmt_sqlstate(&self, statement_id: i64) -> String {
        self.statements.get(&statement_id).map_or_else(
            || "HY000".to_owned(),
            |statement| statement.sqlstate.clone(),
        )
    }

    /// Frees the buffered statement result without closing the statement.
    pub fn stmt_free_result(&mut self, statement_id: i64) -> bool {
        let Some(statement) = self.statements.get_mut(&statement_id) else {
            return false;
        };
        if let Some(result_id) = statement.last_result_id.take() {
            self.results.remove(&result_id);
        }
        true
    }

    /// Closes a statement handle and any buffered result it owns.
    pub fn stmt_close(&mut self, statement_id: i64) -> bool {
        let Some(statement) = self.statements.remove(&statement_id) else {
            return false;
        };
        if let Some(result_id) = statement.last_result_id {
            self.results.remove(&result_id);
        }
        true
    }

    /// Changes the active database for an open connection.
    pub fn select_db(&mut self, id: i64, database: &str) -> Result<(), MysqlError> {
        if database.trim().is_empty() {
            return Err(MysqlError::new(
                MysqlErrorKind::InvalidQuery,
                "MySQL database name must not be empty",
            ));
        }
        let escaped = database.replace('`', "``");
        self.execute(id, &format!("USE `{escaped}`"))
    }

    /// Sets the connection character set for the WordPress mysqli MVP.
    pub fn set_charset(&mut self, id: i64, charset: &str) -> Result<(), MysqlError> {
        let normalized = match charset.to_ascii_lowercase().as_str() {
            "utf8" => "utf8",
            "utf8mb4" => "utf8mb4",
            _ => {
                return Err(MysqlError::new(
                    MysqlErrorKind::InvalidQuery,
                    format!("unsupported MySQL charset for mysqli MVP: {charset}"),
                ));
            }
        };
        self.execute(id, &format!("SET NAMES {normalized}"))
    }

    fn execute(&mut self, id: i64, sql: &str) -> Result<(), MysqlError> {
        let Some(connection) = self.connections.get_mut(&id) else {
            return Err(MysqlError::new(
                MysqlErrorKind::Client,
                "not an open MySQL connection",
            ));
        };
        match connection.connection.execute(sql) {
            Ok(result) => {
                connection.last_errno = 0;
                connection.last_error.clear();
                connection.last_field_count = 0;
                connection.affected_rows = result.affected_rows;
                connection.last_insert_id = result.last_insert_id;
                Ok(())
            }
            Err(error) => {
                connection.last_errno = error.mysql_errno();
                connection.last_error = error.message.clone();
                Err(error)
            }
        }
    }

    /// Returns the last connection error code.
    #[must_use]
    pub fn errno(&self, id: i64) -> i64 {
        self.connections
            .get(&id)
            .map_or(1, |connection| connection.last_errno)
    }

    /// Returns the last connection error message.
    #[must_use]
    pub fn error(&self, id: i64) -> String {
        self.connections.get(&id).map_or_else(
            || "not an open MySQL connection".to_owned(),
            |connection| connection.last_error.clone(),
        )
    }

    /// Returns server information for capability checks.
    #[must_use]
    pub fn server_info(&self, id: i64) -> String {
        self.connections
            .get(&id)
            .map_or_else(String::new, |connection| {
                connection.connection.server_info()
            })
    }

    /// Returns the number of columns in the most recent connection-level result.
    #[must_use]
    pub fn field_count(&self, id: i64) -> i64 {
        self.connections
            .get(&id)
            .map_or(0, |connection| connection.last_field_count)
    }

    /// Returns the last connect error code.
    #[must_use]
    pub const fn connect_errno(&self) -> i64 {
        self.connect_errno
    }

    /// Returns the last connect error message.
    #[must_use]
    pub fn connect_error(&self) -> String {
        self.connect_error.clone()
    }

    /// Fetches one row from a buffered result set.
    pub fn fetch_array(&mut self, id: i64, mode: i64) -> Value {
        let Some(result) = self.results.get_mut(&id) else {
            return Value::Bool(false);
        };
        let Some(row) = result.rows.get(result.offset).cloned() else {
            return Value::Bool(false);
        };
        result.offset = result.offset.saturating_add(1);
        row_to_array(&result.columns, &row, mode)
    }

    /// Resets the next row returned by a buffered result set.
    pub fn data_seek(&mut self, id: i64, offset: usize) -> bool {
        let Some(result) = self.results.get_mut(&id) else {
            return false;
        };
        if offset > result.rows.len() {
            return false;
        }
        result.offset = offset;
        true
    }

    /// Returns field names for a buffered result set.
    #[must_use]
    pub fn field_names(&self, id: i64) -> Vec<String> {
        self.results
            .get(&id)
            .map_or_else(Vec::new, |result| result.columns.clone())
    }

    /// Frees a buffered result set.
    pub fn free_result(&mut self, id: i64) -> bool {
        self.results.remove(&id).is_some()
    }

    /// Returns the number of rows in a buffered result set.
    #[must_use]
    pub fn num_rows(&self, id: i64) -> i64 {
        self.results
            .get(&id)
            .map_or(0, |result| result.rows.len() as i64)
    }

    /// Returns the number of columns in a buffered result set.
    #[must_use]
    pub fn num_fields(&self, id: i64) -> i64 {
        self.results
            .get(&id)
            .map_or(0, |result| result.columns.len() as i64)
    }

    /// Returns the row count affected by the most recent statement.
    #[must_use]
    pub fn affected_rows(&self, id: i64) -> i64 {
        self.connections
            .get(&id)
            .map_or(-1, |connection| connection.affected_rows)
    }

    /// Returns the last auto-increment row id from the connection.
    #[must_use]
    pub fn last_insert_id(&self, id: i64) -> i64 {
        self.connections
            .get(&id)
            .map_or(0, |connection| connection.last_insert_id)
    }

    fn insert_connection(&mut self, connection: MysqlConnectionBackend) -> i64 {
        self.next_connection_id = self.next_connection_id.saturating_add(1).max(1);
        let id = self.next_connection_id;
        self.connections.insert(
            id,
            MysqlRuntimeConnection {
                connection,
                last_errno: 0,
                last_error: String::new(),
                last_field_count: 0,
                affected_rows: 0,
                last_insert_id: 0,
            },
        );
        id
    }

    fn insert_result(&mut self, result: MysqlQueryResult) -> i64 {
        self.next_result_id = self.next_result_id.saturating_add(1).max(1);
        let result_id = self.next_result_id;
        self.results.insert(
            result_id,
            MysqlBufferedResult {
                columns: result.columns,
                rows: result.rows,
                offset: 0,
            },
        );
        result_id
    }

    fn record_stmt_error(&mut self, statement_id: i64, error: &MysqlError) {
        if let Some(statement) = self.statements.get_mut(&statement_id) {
            statement.last_errno = error.mysql_errno();
            statement.last_error = error.message.clone();
            statement.sqlstate = "HY000".to_owned();
        }
    }
}

/// MySQL connection options parsed from a DSN.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MysqlConnectOptions {
    dsn: String,
}

impl MysqlConnectOptions {
    /// Parses a MySQL DSN using the upstream client parser.
    pub fn from_dsn(dsn: impl Into<String>) -> Result<Self, MysqlError> {
        let dsn = dsn.into();
        if dsn.trim().is_empty() {
            return Err(MysqlError::new(
                MysqlErrorKind::MissingDsn,
                "MySQL DSN must not be empty",
            ));
        }
        Opts::from_url(&dsn).map_err(|error| {
            MysqlError::new(
                MysqlErrorKind::InvalidDsn,
                format!("invalid MySQL DSN: {error}"),
            )
        })?;
        Ok(Self { dsn })
    }

    /// Builds a MySQL DSN from mysqli-style connection arguments.
    pub fn from_parts(
        host: &str,
        user: &str,
        password: &str,
        database: Option<&str>,
        port: Option<u16>,
    ) -> Result<Self, MysqlError> {
        let host = if host.trim().is_empty() {
            "localhost"
        } else {
            host.trim()
        };
        let host = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]")
        } else {
            host.to_owned()
        };
        let mut dsn = format!(
            "mysql://{}:{}@{}",
            percent_encode_mysql_url_component(user),
            percent_encode_mysql_url_component(password),
            host
        );
        if let Some(port) = port {
            dsn.push(':');
            dsn.push_str(&port.to_string());
        }
        if let Some(database) = database
            && !database.is_empty()
        {
            dsn.push('/');
            dsn.push_str(&percent_encode_mysql_url_component(database));
        }
        Self::from_dsn(dsn)
    }

    /// Reads `PHRUST_MYSQL_TEST_DSN`.
    #[must_use]
    pub fn from_test_env() -> Option<Result<Self, MysqlError>> {
        match env::var(MYSQL_TEST_DSN_ENV) {
            Ok(value) if !value.trim().is_empty() => Some(Self::from_dsn(value)),
            _ => None,
        }
    }

    fn opts(&self) -> Result<Opts, MysqlError> {
        Opts::from_url(&self.dsn).map_err(|error| {
            MysqlError::new(
                MysqlErrorKind::InvalidDsn,
                format!("invalid MySQL DSN: {error}"),
            )
        })
    }
}

fn percent_encode_mysql_url_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

/// Live MySQL/MariaDB connection handle.
#[derive(Debug)]
pub struct MysqlConnection {
    conn: Conn,
}

impl MysqlConnection {
    /// Opens a live connection. Callers must enforce capability policy before
    /// invoking this constructor.
    pub fn connect(options: &MysqlConnectOptions) -> Result<Self, MysqlError> {
        let conn = Conn::new(options.opts()?).map_err(MysqlError::from_client)?;
        Ok(Self { conn })
    }

    /// Runs a simple text query and buffers all result rows.
    pub fn query(&mut self, sql: &str) -> Result<MysqlQueryResult, MysqlError> {
        if sql.trim().is_empty() {
            return Err(MysqlError::new(
                MysqlErrorKind::InvalidQuery,
                "MySQL query must not be empty",
            ));
        }
        let (columns, rows) = {
            let mut result = self.conn.query_iter(sql).map_err(MysqlError::from_client)?;
            let columns = result
                .columns()
                .as_ref()
                .iter()
                .map(|column| column.name_str().into_owned())
                .collect::<Vec<_>>();
            let mut rows = Vec::new();
            for row in result.by_ref() {
                rows.push(convert_row(row.map_err(MysqlError::from_client)?));
            }
            (columns, rows)
        };
        let affected_rows = self.conn.affected_rows().try_into().unwrap_or(i64::MAX);
        let last_insert_id = self.conn.last_insert_id().try_into().unwrap_or(i64::MAX);
        Ok(MysqlQueryResult {
            columns,
            rows,
            affected_rows,
            last_insert_id,
        })
    }

    /// Runs a SQL statement that does not return rows.
    pub fn execute(&mut self, sql: &str) -> Result<MysqlQueryResult, MysqlError> {
        if sql.trim().is_empty() {
            return Err(MysqlError::new(
                MysqlErrorKind::InvalidQuery,
                "MySQL query must not be empty",
            ));
        }
        self.conn.query_drop(sql).map_err(MysqlError::from_client)?;
        Ok(MysqlQueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
            affected_rows: self.conn.affected_rows().try_into().unwrap_or(i64::MAX),
            last_insert_id: self.conn.last_insert_id().try_into().unwrap_or(i64::MAX),
        })
    }

    /// Validates a prepared statement without executing it.
    pub fn prepare(&mut self, sql: &str) -> Result<(), MysqlError> {
        if sql.trim().is_empty() {
            return Err(MysqlError::new(
                MysqlErrorKind::InvalidQuery,
                "MySQL query must not be empty",
            ));
        }
        let statement = self.conn.prep(sql).map_err(MysqlError::from_client)?;
        self.conn.close(statement).map_err(MysqlError::from_client)
    }

    /// Executes a prepared statement and buffers all returned rows.
    pub fn execute_prepared(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> Result<MysqlQueryResult, MysqlError> {
        if sql.trim().is_empty() {
            return Err(MysqlError::new(
                MysqlErrorKind::InvalidQuery,
                "MySQL query must not be empty",
            ));
        }
        let params = Params::Positional(params.iter().map(value_to_mysql_param).collect());
        let statement = self.conn.prep(sql).map_err(MysqlError::from_client)?;
        let (columns, rows) = {
            let mut result = self
                .conn
                .exec_iter(&statement, params)
                .map_err(MysqlError::from_client)?;
            let columns = result
                .columns()
                .as_ref()
                .iter()
                .map(|column| column.name_str().into_owned())
                .collect::<Vec<_>>();
            let mut rows = Vec::new();
            for row in result.by_ref() {
                rows.push(convert_row(row.map_err(MysqlError::from_client)?));
            }
            (columns, rows)
        };
        self.conn
            .close(statement)
            .map_err(MysqlError::from_client)?;
        Ok(MysqlQueryResult {
            columns,
            rows,
            affected_rows: self.conn.affected_rows().try_into().unwrap_or(i64::MAX),
            last_insert_id: self.conn.last_insert_id().try_into().unwrap_or(i64::MAX),
        })
    }
}

impl MysqlConnectionBackend {
    fn server_info(&self) -> String {
        match self {
            Self::Live(_) => "8.0.0".to_owned(),
            Self::SqliteCompat(_) => "10.11.0-MariaDB".to_owned(),
        }
    }

    fn query(&mut self, sql: &str) -> Result<MysqlQueryResult, MysqlError> {
        match self {
            Self::Live(connection) => connection.query(sql),
            Self::SqliteCompat(connection) => connection.query(sql),
        }
    }

    fn execute(&mut self, sql: &str) -> Result<MysqlQueryResult, MysqlError> {
        match self {
            Self::Live(connection) => connection.execute(sql),
            Self::SqliteCompat(connection) => connection.execute(sql),
        }
    }

    fn prepare(&mut self, sql: &str) -> Result<(), MysqlError> {
        match self {
            Self::Live(connection) => connection.prepare(sql),
            Self::SqliteCompat(connection) => connection.prepare(sql),
        }
    }

    fn execute_prepared(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> Result<MysqlQueryResult, MysqlError> {
        match self {
            Self::Live(connection) => connection.execute_prepared(sql, params),
            Self::SqliteCompat(connection) => connection.execute_prepared(sql, params),
        }
    }
}

/// SQLite-backed compatibility connection for selected mysqli application
/// fixtures. SQL accepted here is SQLite SQL, not a MySQL dialect.
#[derive(Debug)]
pub struct MysqliSqliteCompatConnection {
    conn: SqliteConnection,
}

impl MysqliSqliteCompatConnection {
    fn open_memory() -> Result<Self, MysqlError> {
        let conn = SqliteConnection::open_in_memory_with_flags(
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )
        .map_err(Self::error)?;
        Ok(Self { conn })
    }

    fn query(&mut self, sql: &str) -> Result<MysqlQueryResult, MysqlError> {
        if sql.trim().is_empty() {
            return Err(MysqlError::new(
                MysqlErrorKind::InvalidQuery,
                "MySQL query must not be empty",
            ));
        }
        let mut statement = self.conn.prepare(sql).map_err(Self::error)?;
        let column_count = statement.column_count();
        if column_count == 0 {
            drop(statement);
            return self.execute(sql);
        }
        let columns = statement
            .column_names()
            .into_iter()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let mut rows = Vec::new();
        let mut query = statement.query([]).map_err(Self::error)?;
        while let Some(row) = query.next().map_err(Self::error)? {
            let mut values = Vec::with_capacity(column_count);
            for index in 0..column_count {
                values.push(sqlite_cell(row.get_ref(index).map_err(Self::error)?));
            }
            rows.push(MysqlRow { values });
        }
        Ok(MysqlQueryResult {
            columns,
            rows,
            affected_rows: 0,
            last_insert_id: self.conn.last_insert_rowid(),
        })
    }

    fn execute(&mut self, sql: &str) -> Result<MysqlQueryResult, MysqlError> {
        if sql.trim().is_empty() {
            return Err(MysqlError::new(
                MysqlErrorKind::InvalidQuery,
                "MySQL query must not be empty",
            ));
        }
        let affected_rows = self.conn.execute(sql, []).map_err(Self::error)?;
        Ok(MysqlQueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
            affected_rows: affected_rows.try_into().unwrap_or(i64::MAX),
            last_insert_id: self.conn.last_insert_rowid(),
        })
    }

    fn prepare(&mut self, sql: &str) -> Result<(), MysqlError> {
        if sql.trim().is_empty() {
            return Err(MysqlError::new(
                MysqlErrorKind::InvalidQuery,
                "MySQL query must not be empty",
            ));
        }
        self.conn.prepare(sql).map(drop).map_err(Self::error)
    }

    fn execute_prepared(
        &mut self,
        sql: &str,
        params: &[Value],
    ) -> Result<MysqlQueryResult, MysqlError> {
        if sql.trim().is_empty() {
            return Err(MysqlError::new(
                MysqlErrorKind::InvalidQuery,
                "MySQL query must not be empty",
            ));
        }
        let sqlite_params = params.iter().map(value_to_sqlite_param).collect::<Vec<_>>();
        let mut statement = self.conn.prepare(sql).map_err(Self::error)?;
        let column_count = statement.column_count();
        if column_count == 0 {
            let affected_rows = statement
                .execute(params_from_iter(sqlite_params.iter()))
                .map_err(Self::error)?;
            return Ok(MysqlQueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                affected_rows: affected_rows.try_into().unwrap_or(i64::MAX),
                last_insert_id: self.conn.last_insert_rowid(),
            });
        }
        let columns = statement
            .column_names()
            .into_iter()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        let mut rows = Vec::new();
        let mut query = statement
            .query(params_from_iter(sqlite_params.iter()))
            .map_err(Self::error)?;
        while let Some(row) = query.next().map_err(Self::error)? {
            let mut values = Vec::with_capacity(column_count);
            for index in 0..column_count {
                values.push(sqlite_cell(row.get_ref(index).map_err(Self::error)?));
            }
            rows.push(MysqlRow { values });
        }
        Ok(MysqlQueryResult {
            columns,
            rows,
            affected_rows: 0,
            last_insert_id: self.conn.last_insert_rowid(),
        })
    }

    fn error(error: rusqlite::Error) -> MysqlError {
        MysqlError::new(MysqlErrorKind::Client, error.to_string())
    }
}

/// Buffered result for a simple MySQL text query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MysqlQueryResult {
    /// Column names in result order.
    pub columns: Vec<String>,
    /// Buffered row values.
    pub rows: Vec<MysqlRow>,
    /// Number of rows affected by the statement.
    pub affected_rows: i64,
    /// Last insert row id visible to the connection.
    pub last_insert_id: i64,
}

/// One buffered MySQL row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MysqlRow {
    /// Values in column order.
    pub values: Vec<MysqlCell>,
}

/// Database cell value normalized away from the client crate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MysqlCell {
    /// SQL NULL.
    Null,
    /// Signed or unsigned integer represented losslessly where possible.
    Int(i128),
    /// Floating point value stringified by the client.
    Float(String),
    /// Raw bytes for text/blob values.
    Bytes(Vec<u8>),
    /// Date/time-like value in a stable textual form.
    DateTime(String),
    /// Duration/time value in a stable textual form.
    Time(String),
}

/// MySQL layer error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MysqlError {
    /// Stable classification for diagnostics.
    pub kind: MysqlErrorKind,
    /// Deterministic human-readable message.
    pub message: String,
}

impl MysqlError {
    fn new(kind: MysqlErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    fn from_client(error: mysql::Error) -> Self {
        Self::new(MysqlErrorKind::Client, error.to_string())
    }

    /// Stable MySQL-style error number used by mysqli diagnostics.
    #[must_use]
    pub fn mysql_errno(&self) -> i64 {
        match self.kind {
            MysqlErrorKind::MissingDsn => 2002,
            MysqlErrorKind::InvalidDsn => 2005,
            MysqlErrorKind::InvalidQuery => 1065,
            MysqlErrorKind::Client => 1,
        }
    }

    /// Stable SQLSTATE used by mysqli diagnostics.
    #[must_use]
    pub const fn mysql_sqlstate(&self) -> &'static str {
        match self.kind {
            MysqlErrorKind::MissingDsn
            | MysqlErrorKind::InvalidDsn
            | MysqlErrorKind::InvalidQuery
            | MysqlErrorKind::Client => "HY000",
        }
    }
}

impl fmt::Display for MysqlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for MysqlError {}

/// Stable MySQL error categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MysqlErrorKind {
    /// No DSN was configured for a live operation.
    MissingDsn,
    /// DSN parsing failed.
    InvalidDsn,
    /// Query was invalid before reaching the client.
    InvalidQuery,
    /// Client crate returned an error.
    Client,
}

fn convert_row(row: Row) -> MysqlRow {
    MysqlRow {
        values: row.unwrap().into_iter().map(convert_value).collect(),
    }
}

fn convert_value(value: MysqlValue) -> MysqlCell {
    match value {
        MysqlValue::NULL => MysqlCell::Null,
        MysqlValue::Bytes(bytes) => MysqlCell::Bytes(bytes),
        MysqlValue::Int(value) => MysqlCell::Int(i128::from(value)),
        MysqlValue::UInt(value) => MysqlCell::Int(i128::from(value)),
        MysqlValue::Float(value) => MysqlCell::Float(value.to_string()),
        MysqlValue::Double(value) => MysqlCell::Float(value.to_string()),
        MysqlValue::Date(year, month, day, hour, minute, second, micros) => MysqlCell::DateTime(
            format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}.{micros:06}"),
        ),
        MysqlValue::Time(negative, days, hours, minutes, seconds, micros) => {
            let sign = if negative { "-" } else { "" };
            MysqlCell::Time(format!(
                "{sign}{days} {hours:02}:{minutes:02}:{seconds:02}.{micros:06}"
            ))
        }
    }
}

fn value_to_mysql_param(value: &Value) -> MysqlValue {
    match value {
        Value::Null => MysqlValue::NULL,
        Value::Bool(value) => MysqlValue::Int(i64::from(*value)),
        Value::Int(value) => MysqlValue::Int(*value),
        Value::Float(value) => MysqlValue::Double(value.to_f64()),
        Value::String(value) => MysqlValue::Bytes(value.as_bytes().to_vec()),
        Value::Reference(cell) => value_to_mysql_param(&cell.get()),
        other => MysqlValue::Bytes(
            convert::to_string_php(other)
                .map_or_else(|_| Vec::new(), |value| value.as_bytes().to_vec()),
        ),
    }
}

fn value_to_sqlite_param(value: &Value) -> SqliteValue {
    match value {
        Value::Null => SqliteValue::Null,
        Value::Bool(value) => SqliteValue::Integer(i64::from(*value)),
        Value::Int(value) => SqliteValue::Integer(*value),
        Value::Float(value) => SqliteValue::Real(value.to_f64()),
        Value::String(value) => SqliteValue::Blob(value.as_bytes().to_vec()),
        Value::Reference(cell) => value_to_sqlite_param(&cell.get()),
        other => convert::to_string_php(other).map_or_else(
            |_| SqliteValue::Null,
            |value| SqliteValue::Blob(value.as_bytes().to_vec()),
        ),
    }
}

fn sqlite_cell(value: SqliteValueRef<'_>) -> MysqlCell {
    match value {
        SqliteValueRef::Null => MysqlCell::Null,
        SqliteValueRef::Integer(value) => MysqlCell::Int(i128::from(value)),
        SqliteValueRef::Real(value) => MysqlCell::Float(value.to_string()),
        SqliteValueRef::Text(value) | SqliteValueRef::Blob(value) => {
            MysqlCell::Bytes(value.to_vec())
        }
    }
}

fn row_to_array(columns: &[String], row: &MysqlRow, mode: i64) -> Value {
    let mut array = PhpArray::new();
    if mode & MYSQLI_NUM != 0 {
        for (index, value) in row.values.iter().enumerate() {
            array.insert(ArrayKey::Int(index as i64), cell_to_value(value));
        }
    }
    if mode & MYSQLI_ASSOC != 0 {
        for (name, value) in columns.iter().zip(row.values.iter()) {
            array.insert(
                ArrayKey::String(PhpString::from_bytes(name.as_bytes().to_vec())),
                cell_to_value(value),
            );
        }
    }
    Value::Array(array)
}

fn cell_to_value(cell: &MysqlCell) -> Value {
    match cell {
        MysqlCell::Null => Value::Null,
        MysqlCell::Int(value) => i64::try_from(*value).map_or_else(
            |_| Value::String(PhpString::from(value.to_string().into_bytes())),
            Value::Int,
        ),
        MysqlCell::Float(value) | MysqlCell::DateTime(value) | MysqlCell::Time(value) => {
            Value::String(PhpString::from(value.as_str()))
        }
        MysqlCell::Bytes(value) => Value::string(value.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_mysql_dsn_without_connecting() {
        let options = MysqlConnectOptions::from_dsn("mysql://user:pass@127.0.0.1:3306/db")
            .expect("valid DSN");
        assert_eq!(options.dsn, "mysql://user:pass@127.0.0.1:3306/db");
    }

    #[test]
    fn builds_dsn_from_mysqli_connection_parts() {
        let options = MysqlConnectOptions::from_parts(
            "127.0.0.1",
            "word press",
            "secret/pass",
            Some("wp db"),
            Some(13306),
        )
        .expect("valid mysqli parts");

        assert_eq!(
            options.dsn,
            "mysql://word%20press:secret%2Fpass@127.0.0.1:13306/wp%20db"
        );
    }

    #[test]
    fn rejects_empty_dsn() {
        let error = MysqlConnectOptions::from_dsn(" ").expect_err("empty DSN fails");
        assert_eq!(error.kind, MysqlErrorKind::MissingDsn);
    }

    #[test]
    fn rejects_non_mysql_dsn() {
        let error = MysqlConnectOptions::from_dsn("postgres://localhost/db")
            .expect_err("wrong scheme fails");
        assert_eq!(error.kind, MysqlErrorKind::InvalidDsn);
    }

    #[test]
    fn converts_client_values_to_stable_cells() {
        assert_eq!(convert_value(MysqlValue::NULL), MysqlCell::Null);
        assert_eq!(convert_value(MysqlValue::Int(-7)), MysqlCell::Int(-7));
        assert_eq!(convert_value(MysqlValue::UInt(7)), MysqlCell::Int(7));
        assert_eq!(
            convert_value(MysqlValue::Bytes(b"alpha".to_vec())),
            MysqlCell::Bytes(b"alpha".to_vec())
        );
    }

    #[test]
    fn live_query_smoke_skips_without_dsn() {
        let Some(options) = MysqlConnectOptions::from_test_env() else {
            return;
        };
        let options = options.expect("configured DSN should parse");
        let mut connection = MysqlConnection::connect(&options).expect("connect to MySQL");
        let result = connection
            .query("SELECT 1 AS one")
            .expect("run simple query");
        assert_eq!(result.columns, vec!["one"]);
        assert_eq!(result.rows.len(), 1);
    }

    #[test]
    fn sqlite_compat_query_fetch_error_and_status_are_tracked() {
        let mut state = MysqlState::default();
        let id = state
            .connect_sqlite_compat()
            .expect("open compatibility backend");

        assert_eq!(
            state
                .query(
                    id,
                    "CREATE TABLE items (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT)"
                )
                .expect("create table"),
            None
        );
        assert_eq!(
            state
                .query(id, "INSERT INTO items (name) VALUES ('alpha')")
                .expect("insert row"),
            None
        );
        assert_eq!(state.affected_rows(id), 1);
        assert_eq!(state.last_insert_id(id), 1);

        let result_id = state
            .query(id, "SELECT id, name FROM items ORDER BY id")
            .expect("select rows")
            .expect("row result");
        assert_eq!(state.num_rows(result_id), 1);
        assert_eq!(state.num_fields(result_id), 2);
        let row = state.fetch_array(result_id, MYSQLI_ASSOC);
        let Value::Array(row) = row else {
            panic!("expected mysqli row array");
        };
        assert_eq!(
            row.get(&ArrayKey::String(PhpString::from("name"))),
            Some(&Value::string("alpha"))
        );

        assert!(state.query(id, "SELECT missing FROM items").is_err());
        assert_ne!(state.errno(id), 0);
        assert!(!state.error(id).is_empty());
    }
}
