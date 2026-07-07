#![allow(unused_imports, dead_code)]

use crate::{ArrayKey, PhpArray, PhpString, Value, convert};
use std::collections::HashMap;

pub const POSTGRES_TEST_DSN_ENV: &str = "PHRUST_POSTGRES_TEST_DSN";
pub const PGSQL_ASSOC: i64 = 1;
pub const PGSQL_NUM: i64 = 2;
pub const PGSQL_BOTH: i64 = PGSQL_ASSOC | PGSQL_NUM;

#[derive(Debug, Default)]
pub struct PostgresState {
    next_connection_id: i64,
    next_result_id: i64,
    default_connection_id: Option<i64>,
    connections: HashMap<i64, PostgresRuntimeConnection>,
    results: HashMap<i64, PostgresBufferedResult>,
    prepared: HashMap<(i64, String), String>,
    connect_errno: i64,
    connect_error: String,
}

#[derive(Debug)]
struct PostgresRuntimeConnection {
    connection: PostgresConnection,
    last_sqlstate: String,
    last_error: String,
    affected_rows: i64,
}

#[derive(Debug, Default)]
struct PostgresBufferedResult {
    connection_id: i64,
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
    affected_rows: i64,
}

impl PostgresState {
    pub fn connect(&mut self, _options: &PostgresConnectOptions) -> Result<i64, PostgresError> {
        Err(PostgresError::new(PostgresErrorKind::Client, "not available on wasm"))
    }
    pub fn close(&mut self, _id: i64) -> bool { false }
    pub const fn default_connection(&self) -> Option<i64> { self.default_connection_id }
    pub fn query(&mut self, _id: i64, _sql: &str) -> Result<Option<i64>, PostgresError> {
        Err(PostgresError::new(PostgresErrorKind::Client, "not available on wasm"))
    }
    pub fn exec_changes(&mut self, _id: i64, _sql: &str) -> Result<i64, PostgresError> {
        Err(PostgresError::new(PostgresErrorKind::Client, "not available on wasm"))
    }
    pub fn execute_prepared(&mut self, _id: i64, _query: &str, _params: &[Value]) -> Result<Option<i64>, PostgresError> {
        Err(PostgresError::new(PostgresErrorKind::Client, "not available on wasm"))
    }
    pub fn prepare_named(&mut self, _id: i64, _query: &str) -> Result<String, PostgresError> {
        Err(PostgresError::new(PostgresErrorKind::Client, "not available on wasm"))
    }
    pub fn execute_named(&mut self, _id: i64, _name: &str, _params: &[Value]) -> Result<Option<i64>, PostgresError> {
        Err(PostgresError::new(PostgresErrorKind::Client, "not available on wasm"))
    }
    pub fn fetch_array(&mut self, _result_id: i64, _mode: i64) -> Value { Value::Null }
    pub fn fetch_array_at(&mut self, _result_id: i64, _index: i64, _mode: i64) -> Value { Value::Null }
    pub fn free_result(&mut self, _result_id: i64) -> bool { false }
    pub fn empty_result(&mut self, _result_id: i64) -> bool { false }
    pub fn num_rows(&self, _result_id: i64) -> i64 { 0 }
    pub fn num_fields(&self, _result_id: i64) -> i64 { 0 }
    pub fn affected_rows(&self, _connection_id: i64) -> i64 { 0 }
    pub fn affected_result_rows(&self, _result_id: i64) -> i64 { 0 }
    pub fn result_connection_id(&self, _result_id: i64) -> i64 { 0 }
    pub fn field_names(&self, _result_id: i64) -> Vec<String> { Vec::new() }
    pub fn error(&self, _connection_id: i64) -> String { "not available on wasm".to_owned() }
    pub fn sqlstate(&self, _connection_id: i64) -> String { "00000".to_owned() }
    pub fn connect_errno(&self) -> i64 { self.connect_errno }
    pub fn connect_error(&self) -> String { self.connect_error.clone() }
    pub fn default_connection_id(&self) -> Option<i64> { self.default_connection_id }
}

#[derive(Clone, Debug)]
pub enum PostgresField { Index(usize), Name(String) }

#[derive(Clone, Debug, Default)]
pub struct PostgresRow {
    values: Vec<Value>,
}

#[derive(Debug)]
pub struct PostgresConnection {
    _private: (),
}

impl PostgresConnection {
    pub fn connect(_options: &PostgresConnectOptions) -> Result<Self, PostgresError> {
        Err(PostgresError::new(PostgresErrorKind::Client, "not available on wasm"))
    }
}

#[derive(Clone, Debug, Default)]
pub struct PostgresConnectOptions {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub dbname: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
}

impl PostgresConnectOptions {
    pub fn from_dsn(_dsn: impl Into<String>) -> Result<Self, String> {
        Err("not available on wasm".to_owned())
    }
    pub fn from_test_env() -> Option<Result<Self, PostgresError>> {
        None
    }
}

#[derive(Clone, Debug)]
pub struct PostgresQueryResult {
    _private: (),
}

#[derive(Clone, Debug)]
pub struct PostgresError {
    pub kind: PostgresErrorKind,
    pub message: String,
    pub sqlstate: String,
}

impl PostgresError {
    pub fn new(kind: PostgresErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into(), sqlstate: String::new() }
    }
    pub fn pg_errno(&self) -> i64 { match self.kind { PostgresErrorKind::MissingDsn => 1, _ => 2 } }
}

impl std::fmt::Display for PostgresError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for PostgresError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PostgresErrorKind { MissingDsn, InvalidQuery, Client }
