//! Capability-gated MySQL/MariaDB client layer.

use crate::{ArrayKey, PhpArray, PhpString, Value};
use mysql::{Conn, Opts, Row, Value as MysqlValue, prelude::Queryable};
use std::collections::HashMap;
use std::env;
use std::fmt;

/// Environment variable that enables live MySQL/MariaDB tests.
pub const MYSQL_TEST_DSN_ENV: &str = "PHRUST_MYSQL_TEST_DSN";

/// `mysqli_fetch_array()` associative columns.
pub const MYSQLI_ASSOC: i64 = 1;
/// `mysqli_fetch_array()` numeric columns.
pub const MYSQLI_NUM: i64 = 2;
/// `mysqli_fetch_array()` associative and numeric columns.
pub const MYSQLI_BOTH: i64 = MYSQLI_ASSOC | MYSQLI_NUM;

#[derive(Clone, Debug, Eq, PartialEq)]
struct MysqlBufferedResult {
    columns: Vec<String>,
    rows: Vec<MysqlRow>,
    offset: usize,
}

#[derive(Debug)]
struct MysqlRuntimeConnection {
    connection: MysqlConnection,
    last_errno: i64,
    last_error: String,
}

/// Request-local MySQL/MariaDB connections and buffered result sets.
#[derive(Default)]
pub struct MysqlState {
    next_connection_id: i64,
    connections: HashMap<i64, MysqlRuntimeConnection>,
    next_result_id: i64,
    results: HashMap<i64, MysqlBufferedResult>,
    connect_errno: i64,
    connect_error: String,
}

impl fmt::Debug for MysqlState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MysqlState")
            .field("next_connection_id", &self.next_connection_id)
            .field("connections", &self.connections.keys().collect::<Vec<_>>())
            .field("next_result_id", &self.next_result_id)
            .field("results", &self.results.keys().collect::<Vec<_>>())
            .field("connect_errno", &self.connect_errno)
            .field("connect_error", &self.connect_error)
            .finish()
    }
}

impl MysqlState {
    /// Records a deterministic connection failure without opening a socket.
    pub fn record_connect_error(&mut self, errno: i64, message: impl Into<String>) {
        self.connect_errno = errno;
        self.connect_error = message.into();
    }

    /// Opens a live MySQL/MariaDB connection from an explicit DSN.
    pub fn connect(&mut self, options: &MysqlConnectOptions) -> Result<i64, MysqlError> {
        match MysqlConnection::connect(options) {
            Ok(connection) => {
                self.connect_errno = 0;
                self.connect_error.clear();
                self.next_connection_id = self.next_connection_id.saturating_add(1).max(1);
                let id = self.next_connection_id;
                self.connections.insert(
                    id,
                    MysqlRuntimeConnection {
                        connection,
                        last_errno: 0,
                        last_error: String::new(),
                    },
                );
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
        self.connections.remove(&id).is_some()
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
                if result.columns.is_empty() {
                    return Ok(None);
                }
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
                Ok(Some(result_id))
            }
            Err(error) => {
                connection.last_errno = error.mysql_errno();
                connection.last_error = error.message.clone();
                Err(error)
            }
        }
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
            Ok(()) => {
                connection.last_errno = 0;
                connection.last_error.clear();
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
        Ok(MysqlQueryResult { columns, rows })
    }

    /// Runs a SQL statement that does not return rows.
    pub fn execute(&mut self, sql: &str) -> Result<(), MysqlError> {
        if sql.trim().is_empty() {
            return Err(MysqlError::new(
                MysqlErrorKind::InvalidQuery,
                "MySQL query must not be empty",
            ));
        }
        self.conn.query_drop(sql).map_err(MysqlError::from_client)
    }
}

/// Buffered result for a simple MySQL text query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MysqlQueryResult {
    /// Column names in result order.
    pub columns: Vec<String>,
    /// Buffered row values.
    pub rows: Vec<MysqlRow>,
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

    fn mysql_errno(&self) -> i64 {
        match self.kind {
            MysqlErrorKind::MissingDsn => 2002,
            MysqlErrorKind::InvalidDsn => 2005,
            MysqlErrorKind::InvalidQuery => 1065,
            MysqlErrorKind::Client => 1,
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
}
