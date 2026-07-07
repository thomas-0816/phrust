#![allow(unused_imports, dead_code)]

use crate::{PhpString, Value};
use std::collections::HashMap;

pub const MYSQL_TEST_DSN_ENV: &str = "PHRUST_MYSQL_TEST_DSN";
pub const MYSQLI_SQLITE_COMPAT_ENV: &str = "PHRUST_MYSQLI_SQLITE_COMPAT";
pub const MYSQLND_CLIENT_INFO: &str = "mysqlnd 8.5.7";
pub const MYSQLND_CLIENT_VERSION: i64 = 80507;
pub const MYSQLI_ASSOC: i64 = 1;
pub const MYSQLI_NUM: i64 = 2;
pub const MYSQLI_BOTH: i64 = MYSQLI_ASSOC | MYSQLI_NUM;
pub const MYSQLI_REPORT_OFF: i64 = 0;
pub const MYSQLI_REPORT_ERROR: i64 = 1;
pub const MYSQLI_REPORT_STRICT: i64 = 2;
pub const MYSQLI_REPORT_INDEX: i64 = 4;

#[derive(Clone, Debug, Default)]
pub struct MysqlState {
    connections: HashMap<i64, MysqliConnectionData>,
    next_id: i64,
    report_mode: i64,
}

#[derive(Clone, Debug)]
struct MysqliConnectionData {
    _private: (),
}

impl MysqlState {
    pub fn connect(&mut self, _options: &MysqlConnectOptions) -> Result<i64, MysqlError> {
        Err(MysqlError { kind: MysqlErrorKind::Connection, message: "not available on wasm".into(), sqlstate: "HY000".into() })
    }
    pub fn close(&mut self, _id: i64) -> bool { false }
    pub fn change_user(&mut self, _id: i64, _user: &str, _password: &str, _database: &str) -> Result<(), MysqlError> {
        Err(MysqlError { kind: MysqlErrorKind::Connection, message: "not available on wasm".into(), sqlstate: "HY000".into() })
    }
    pub fn set_charset(&mut self, _id: i64, _charset: &str) -> Result<(), MysqlError> {
        Err(MysqlError { kind: MysqlErrorKind::Connection, message: "not available on wasm".into(), sqlstate: "HY000".into() })
    }
    pub fn select_db(&mut self, _id: i64, _db: &str) -> Result<(), MysqlError> {
        Err(MysqlError { kind: MysqlErrorKind::Connection, message: "not available on wasm".into(), sqlstate: "HY000".into() })
    }
    pub fn query(&mut self, _id: i64, _sql: &str) -> Result<Option<i64>, MysqlError> {
        Err(MysqlError { kind: MysqlErrorKind::Query, message: "not available on wasm".into(), sqlstate: "HY000".into() })
    }
    pub fn real_query(&mut self, _id: i64, _sql: &str) -> bool { false }
    pub fn affected_rows(&self, _id: i64) -> i64 { 0 }
    pub fn insert_id(&self, _id: i64) -> i64 { 0 }
    pub fn num_rows(&self, _id: i64) -> i64 { 0 }
    pub fn num_fields(&self, _id: i64) -> i64 { 0 }
    pub fn field_count(&self, _id: i64) -> i64 { 0 }
    pub fn error(&self, _id: i64) -> String { "not available on wasm".into() }
    pub fn errno(&self, _id: i64) -> i64 { 2000 }
    pub fn sqlstate(&self, _id: i64) -> String { "HY000".into() }
    pub fn fetch_array(&mut self, _id: i64, _mode: i64) -> Value { Value::Null }
    pub fn fetch_all(&mut self, _id: i64, _mode: i64) -> Value { Value::Null }
    pub fn fetch_field_direct(&self, _id: i64, _result_id: i64, _field: i64) -> Value { Value::Null }
    pub fn fetch_fields(&self, _id: i64, _result_id: i64) -> Value { Value::Null }
    pub fn free_result(&mut self, _id: i64) -> bool { false }
    pub fn data_seek(&mut self, _id: i64, _result_id: i64, _offset: i64) -> bool { false }
    pub fn more_results(&self, _id: i64) -> bool { false }
    pub fn next_result(&mut self, _id: i64) -> bool { false }
    pub fn autocommit(&mut self, _id: i64, _mode: bool) -> bool { false }
    pub fn commit(&mut self, _id: i64) -> bool { false }
    pub fn rollback(&mut self, _id: i64) -> bool { false }
    pub fn report_mode(&self) -> i64 { self.report_mode }
    pub fn set_report_mode(&mut self, mode: i64) { self.report_mode = mode; }
    pub fn escape_string(&self, _text: &str) -> String { String::new() }
    pub fn get_server_info(&self, _id: i64) -> String { "not available on wasm".into() }
    pub fn get_host_info(&self, _id: i64) -> String { String::new() }
    pub fn get_proto_info(&self, _id: i64) -> i64 { 0 }
    pub fn thread_id(&self, _id: i64) -> i64 { 0 }
    pub fn ping(&mut self, _id: i64) -> bool { false }
    pub fn refresh(&mut self, _id: i64) -> bool { false }
    pub fn kill(&mut self, _id: i64, _connection_id: i64) -> bool { false }
    pub fn debug(&mut self, _debug: &str) { }
    pub fn stat(&self, _id: i64) -> String { String::new() }

    pub fn set_report_flags(&mut self, _flags: i64) {}
    pub fn record_connect_error(&mut self, _errno: i64, _message: impl Into<String>) {}
    pub fn connect_sqlite_compat(&mut self) -> Result<i64, MysqlError> {
        Err(MysqlError { kind: MysqlErrorKind::Connection, message: "not available on wasm".into(), sqlstate: "HY000".into() })
    }
    pub fn stmt_init(&mut self, _connection_id: i64) -> Result<i64, MysqlError> {
        Err(MysqlError { kind: MysqlErrorKind::PreparedStatement, message: "not available on wasm".into(), sqlstate: "HY000".into() })
    }
    pub fn prepare_statement(&mut self, _connection_id: i64, _sql: &str) -> Result<i64, MysqlError> {
        Err(MysqlError { kind: MysqlErrorKind::PreparedStatement, message: "not available on wasm".into(), sqlstate: "HY000".into() })
    }
    pub fn stmt_prepare(&mut self, _statement_id: i64, _sql: &str) -> Result<(), MysqlError> {
        Err(MysqlError { kind: MysqlErrorKind::PreparedStatement, message: "not available on wasm".into(), sqlstate: "HY000".into() })
    }
    pub fn stmt_execute(&mut self, _statement_id: i64, _params: &[Value]) -> Result<bool, MysqlError> {
        Err(MysqlError { kind: MysqlErrorKind::PreparedStatement, message: "not available on wasm".into(), sqlstate: "HY000".into() })
    }
    pub fn stmt_result(&self, _statement_id: i64) -> Option<i64> { None }
    pub fn stmt_fetch_row(&mut self, _statement_id: i64) -> Option<Vec<Value>> { None }
    pub fn stmt_num_rows(&self, _statement_id: i64) -> i64 { 0 }
    pub fn stmt_affected_rows(&self, _statement_id: i64) -> i64 { 0 }
    pub fn stmt_insert_id(&self, _statement_id: i64) -> i64 { 0 }
    pub fn stmt_errno(&self, _statement_id: i64) -> i64 { 2000 }
    pub fn stmt_error(&self, _statement_id: i64) -> String { "not available on wasm".into() }
    pub fn stmt_sqlstate(&self, _statement_id: i64) -> String { "HY000".into() }
    pub fn stmt_free_result(&mut self, _statement_id: i64) -> bool { false }
    pub fn stmt_close(&mut self, _statement_id: i64) -> bool { false }
    pub fn exec_changes(&mut self, _id: i64, _sql: &str) -> Result<i64, MysqlError> {
        Err(MysqlError { kind: MysqlErrorKind::Query, message: "not available on wasm".into(), sqlstate: "HY000".into() })
    }
    pub fn field_names(&self, _id: i64) -> Vec<String> { Vec::new() }
    pub fn server_info(&self, _id: i64) -> String { "not available on wasm".into() }
    pub const fn report_flags(&self) -> i64 { self.report_mode }
    pub fn last_insert_id(&self, _id: i64) -> i64 { 0 }
    pub const fn connect_errno(&self) -> i64 { 2000 }
    pub fn connect_error(&self) -> String { "not available on wasm".into() }
}

#[derive(Clone, Debug, Default)]
pub struct MysqlConnectOptions {
    pub host: Option<String>,
    pub port: Option<u16>,
    pub dbname: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub socket: Option<String>,
    pub flags: i64,
}

impl MysqlConnectOptions {
    pub fn parse(_ini: &str) -> Result<Self, String> { Err("not available on wasm".into()) }
    pub fn from_test_env() -> Option<Result<Self, MysqlError>> { None }
    pub fn from_parts(
        _host: &str,
        _user: &str,
        _password: &str,
        _database: Option<&str>,
        _port: Option<u16>,
    ) -> Result<Self, MysqlError> {
        Err(MysqlError { kind: MysqlErrorKind::Connection, message: "not available on wasm".into(), sqlstate: "HY000".into() })
    }
}

#[derive(Clone, Debug)]
pub struct MysqlConnection { _private: () }

#[derive(Clone, Debug, Default)]
pub struct MysqliSqliteCompatConnection { _private: () }

#[derive(Clone, Debug)]
pub struct MysqlQueryResult { _private: () }

#[derive(Clone, Debug)]
pub struct MysqlRow { _private: () }

#[derive(Clone, Debug)]
pub enum MysqlCell { Null, Int(i64), Float(f64), String(PhpString), Blob(Vec<u8>) }

#[derive(Clone, Debug)]
pub struct MysqlError {
    pub kind: MysqlErrorKind,
    pub message: String,
    pub sqlstate: String,
}

impl MysqlError {
    pub fn mysql_errno(&self) -> i64 {
        match self.kind {
            MysqlErrorKind::Connection => 2002,
            MysqlErrorKind::Query => 1064,
            MysqlErrorKind::PreparedStatement => 1064,
            MysqlErrorKind::Transaction => 2000,
            MysqlErrorKind::Server => 2000,
        }
    }
    pub fn mysql_sqlstate(&self) -> &str {
        &self.sqlstate
    }
}

impl std::fmt::Display for MysqlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MysqlErrorKind { Connection, Query, PreparedStatement, Transaction, Server }
