//! Capability-gated PostgreSQL client layer.

use crate::{ArrayKey, PhpArray, PhpString, Value, convert};
use postgres::types::{ToSql, Type};
use postgres::{Client, NoTls, Row};
use std::collections::HashMap;
use std::env;
use std::fmt;

/// Environment variable that enables live PostgreSQL tests.
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

impl PostgresState {
    pub fn connect(&mut self, options: &PostgresConnectOptions) -> Result<i64, PostgresError> {
        match PostgresConnection::connect(options) {
            Ok(connection) => {
                let id = self.insert_connection(connection);
                self.default_connection_id = Some(id);
                self.connect_errno = 0;
                self.connect_error.clear();
                Ok(id)
            }
            Err(error) => {
                self.connect_errno = error.pg_errno();
                self.connect_error = error.message.clone();
                Err(error)
            }
        }
    }

    pub fn close(&mut self, id: i64) -> bool {
        let removed = self.connections.remove(&id).is_some();
        if removed {
            self.results.retain(|_, result| result.connection_id != id);
            self.prepared
                .retain(|(connection_id, _), _| *connection_id != id);
            if self.default_connection_id == Some(id) {
                self.default_connection_id = self.connections.keys().next().copied();
            }
        }
        removed
    }

    #[must_use]
    pub const fn default_connection(&self) -> Option<i64> {
        self.default_connection_id
    }

    pub fn query(&mut self, id: i64, sql: &str) -> Result<Option<i64>, PostgresError> {
        let Some(connection) = self.connections.get_mut(&id) else {
            return Err(PostgresError::new(
                PostgresErrorKind::Client,
                "not an open PostgreSQL connection",
            ));
        };
        match connection.connection.query(sql, &[]) {
            Ok(result) => {
                connection.last_sqlstate = "00000".to_owned();
                connection.last_error.clear();
                connection.affected_rows = result.affected_rows;
                if result.columns.is_empty() {
                    return Ok(None);
                }
                Ok(Some(self.insert_result(id, result)))
            }
            Err(error) => {
                connection.record_error(&error);
                Err(error)
            }
        }
    }

    pub fn query_params(
        &mut self,
        id: i64,
        sql: &str,
        params: &[Value],
    ) -> Result<Option<i64>, PostgresError> {
        self.execute_prepared(id, sql, params)
    }

    pub fn exec_changes(&mut self, id: i64, sql: &str) -> Result<i64, PostgresError> {
        let Some(connection) = self.connections.get_mut(&id) else {
            return Err(PostgresError::new(
                PostgresErrorKind::Client,
                "not an open PostgreSQL connection",
            ));
        };
        match connection.connection.execute(sql, &[]) {
            Ok(result) => {
                connection.last_sqlstate = "00000".to_owned();
                connection.last_error.clear();
                connection.affected_rows = result.affected_rows;
                Ok(result.affected_rows)
            }
            Err(error) => {
                connection.record_error(&error);
                Err(error)
            }
        }
    }

    pub fn execute_prepared(
        &mut self,
        id: i64,
        sql: &str,
        params: &[Value],
    ) -> Result<Option<i64>, PostgresError> {
        let Some(connection) = self.connections.get_mut(&id) else {
            return Err(PostgresError::new(
                PostgresErrorKind::Client,
                "not an open PostgreSQL connection",
            ));
        };
        let params = postgres_params(params)?;
        let refs = params
            .iter()
            .map(PostgresParam::as_tosql)
            .collect::<Vec<_>>();
        let result = if postgres_query_returns_rows(sql) {
            connection.connection.query(sql, &refs)
        } else {
            connection.connection.execute(sql, &refs)
        };
        match result {
            Ok(result) => {
                connection.last_sqlstate = "00000".to_owned();
                connection.last_error.clear();
                connection.affected_rows = result.affected_rows;
                if result.columns.is_empty() {
                    return Ok(None);
                }
                Ok(Some(self.insert_result(id, result)))
            }
            Err(error) => {
                connection.record_error(&error);
                Err(error)
            }
        }
    }

    pub fn prepare_named(&mut self, id: i64, name: &str, sql: &str) -> Result<(), PostgresError> {
        if !self.connections.contains_key(&id) {
            return Err(PostgresError::new(
                PostgresErrorKind::Client,
                "not an open PostgreSQL connection",
            ));
        }
        if name.trim().is_empty() {
            return Err(PostgresError::new(
                PostgresErrorKind::InvalidQuery,
                "PostgreSQL statement name must not be empty",
            ));
        }
        if sql.trim().is_empty() {
            return Err(PostgresError::new(
                PostgresErrorKind::InvalidQuery,
                "PostgreSQL query must not be empty",
            ));
        }
        self.prepared.insert((id, name.to_owned()), sql.to_owned());
        Ok(())
    }

    pub fn execute_named(
        &mut self,
        id: i64,
        name: &str,
        params: &[Value],
    ) -> Result<Option<i64>, PostgresError> {
        let Some(sql) = self.prepared.get(&(id, name.to_owned())).cloned() else {
            return Err(PostgresError::new(
                PostgresErrorKind::InvalidQuery,
                "prepared PostgreSQL statement does not exist",
            ));
        };
        self.execute_prepared(id, &sql, params)
    }

    pub fn fetch_array(&mut self, id: i64, mode: i64) -> Value {
        self.fetch_array_at(id, None, mode)
    }

    pub fn fetch_array_at(&mut self, id: i64, offset: Option<usize>, mode: i64) -> Value {
        let Some(result) = self.results.get_mut(&id) else {
            return Value::Bool(false);
        };
        let row_offset = offset.unwrap_or(result.offset);
        let Some(row) = result.rows.get(row_offset).cloned() else {
            return Value::Bool(false);
        };
        result.offset = row_offset.saturating_add(1);
        row_to_array(&result.columns, &row, mode)
    }

    pub fn result_value(&self, id: i64, row: usize, field: PostgresField) -> Value {
        let Some(result) = self.results.get(&id) else {
            return Value::Bool(false);
        };
        let Some(row) = result.rows.get(row) else {
            return Value::Bool(false);
        };
        let index = match field {
            PostgresField::Index(index) => index,
            PostgresField::Name(name) => result
                .columns
                .iter()
                .position(|column| column == &name)
                .unwrap_or(usize::MAX),
        };
        row.values.get(index).cloned().unwrap_or(Value::Bool(false))
    }

    pub fn free_result(&mut self, id: i64) -> bool {
        self.results.remove(&id).is_some()
    }

    pub fn empty_result(&mut self, connection_id: i64, affected_rows: i64) -> i64 {
        self.insert_result(connection_id, PostgresQueryResult::empty(affected_rows))
    }

    #[must_use]
    pub fn num_rows(&self, id: i64) -> i64 {
        self.results
            .get(&id)
            .map_or(0, |result| result.rows.len() as i64)
    }

    #[must_use]
    pub fn num_fields(&self, id: i64) -> i64 {
        self.results
            .get(&id)
            .map_or(0, |result| result.columns.len() as i64)
    }

    #[must_use]
    pub fn affected_rows(&self, id: i64) -> i64 {
        self.connections
            .get(&id)
            .map_or(-1, |connection| connection.affected_rows)
    }

    #[must_use]
    pub fn affected_result_rows(&self, id: i64) -> i64 {
        self.results.get(&id).map_or(-1, |result| {
            result.affected_rows.max(result.rows.len() as i64)
        })
    }

    #[must_use]
    pub fn result_connection_id(&self, id: i64) -> Option<i64> {
        self.results.get(&id).map(|result| result.connection_id)
    }

    #[must_use]
    pub fn result_error(&self, id: i64) -> Option<String> {
        self.results
            .get(&id)
            .map(|result| result.error_message.clone())
    }

    #[must_use]
    pub fn field_names(&self, id: i64) -> Vec<String> {
        self.results
            .get(&id)
            .map_or_else(Vec::new, |result| result.columns.clone())
    }

    #[must_use]
    pub fn error(&self, id: i64) -> String {
        self.connections.get(&id).map_or_else(
            || "not an open PostgreSQL connection".to_owned(),
            |connection| connection.last_error.clone(),
        )
    }

    #[must_use]
    pub fn sqlstate(&self, id: i64) -> String {
        self.connections.get(&id).map_or_else(
            || "HY000".to_owned(),
            |connection| connection.last_sqlstate.clone(),
        )
    }

    #[must_use]
    pub const fn connect_errno(&self) -> i64 {
        self.connect_errno
    }

    #[must_use]
    pub fn connect_error(&self) -> String {
        self.connect_error.clone()
    }

    fn insert_connection(&mut self, connection: PostgresConnection) -> i64 {
        self.next_connection_id = self.next_connection_id.saturating_add(1).max(1);
        let id = self.next_connection_id;
        self.connections.insert(
            id,
            PostgresRuntimeConnection {
                connection,
                last_sqlstate: "00000".to_owned(),
                last_error: String::new(),
                affected_rows: 0,
            },
        );
        id
    }

    fn insert_result(&mut self, connection_id: i64, result: PostgresQueryResult) -> i64 {
        self.next_result_id = self.next_result_id.saturating_add(1).max(1);
        let id = self.next_result_id;
        self.results.insert(
            id,
            PostgresBufferedResult {
                connection_id,
                columns: result.columns,
                rows: result.rows,
                affected_rows: result.affected_rows,
                error_message: result.error_message,
                offset: 0,
            },
        );
        id
    }
}

#[derive(Debug)]
struct PostgresRuntimeConnection {
    connection: PostgresConnection,
    last_sqlstate: String,
    last_error: String,
    affected_rows: i64,
}

impl PostgresRuntimeConnection {
    fn record_error(&mut self, error: &PostgresError) {
        self.last_sqlstate = error.sqlstate.clone();
        self.last_error = error.message.clone();
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PostgresBufferedResult {
    connection_id: i64,
    columns: Vec<String>,
    rows: Vec<PostgresRow>,
    affected_rows: i64,
    error_message: String,
    offset: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PostgresField {
    Index(usize),
    Name(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostgresRow {
    values: Vec<Value>,
}

pub struct PostgresConnection {
    client: Client,
}

impl fmt::Debug for PostgresConnection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PostgresConnection")
            .finish_non_exhaustive()
    }
}

impl PostgresConnection {
    pub fn connect(options: &PostgresConnectOptions) -> Result<Self, PostgresError> {
        let client = Client::connect(&options.dsn, NoTls).map_err(PostgresError::from_client)?;
        Ok(Self { client })
    }

    fn query(
        &mut self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<PostgresQueryResult, PostgresError> {
        if sql.trim().is_empty() {
            return Err(PostgresError::new(
                PostgresErrorKind::InvalidQuery,
                "PostgreSQL query must not be empty",
            ));
        }
        let rows = self
            .client
            .query(sql, params)
            .map_err(PostgresError::from_client)?;
        Ok(PostgresQueryResult::from_rows(rows, 0))
    }

    fn execute(
        &mut self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<PostgresQueryResult, PostgresError> {
        if sql.trim().is_empty() {
            return Err(PostgresError::new(
                PostgresErrorKind::InvalidQuery,
                "PostgreSQL query must not be empty",
            ));
        }
        let affected_rows = self
            .client
            .execute(sql, params)
            .map_err(PostgresError::from_client)?
            .try_into()
            .unwrap_or(i64::MAX);
        Ok(PostgresQueryResult::empty(affected_rows))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostgresConnectOptions {
    dsn: String,
}

impl PostgresConnectOptions {
    pub fn from_dsn(dsn: impl Into<String>) -> Result<Self, PostgresError> {
        let dsn = dsn.into();
        if dsn.trim().is_empty() {
            return Err(PostgresError::new(
                PostgresErrorKind::MissingDsn,
                "PostgreSQL DSN must not be empty",
            ));
        }
        Ok(Self { dsn })
    }

    #[must_use]
    pub fn from_test_env() -> Option<Result<Self, PostgresError>> {
        match env::var(POSTGRES_TEST_DSN_ENV) {
            Ok(value) if !value.trim().is_empty() => Some(Self::from_dsn(value)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostgresQueryResult {
    columns: Vec<String>,
    rows: Vec<PostgresRow>,
    affected_rows: i64,
    error_message: String,
}

impl PostgresQueryResult {
    fn empty(affected_rows: i64) -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
            affected_rows,
            error_message: String::new(),
        }
    }

    fn from_rows(rows: Vec<Row>, affected_rows: i64) -> Self {
        let columns = rows.first().map_or_else(Vec::new, |row| {
            row.columns()
                .iter()
                .map(|column| column.name().to_owned())
                .collect()
        });
        let rows = rows.into_iter().map(convert_row).collect();
        Self {
            columns,
            rows,
            affected_rows,
            error_message: String::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostgresError {
    pub kind: PostgresErrorKind,
    pub message: String,
    pub sqlstate: String,
}

impl PostgresError {
    fn new(kind: PostgresErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            sqlstate: "HY000".to_owned(),
        }
    }

    fn from_client(error: postgres::Error) -> Self {
        if let Some(db_error) = error.as_db_error() {
            return Self {
                kind: PostgresErrorKind::Client,
                message: db_error.message().to_owned(),
                sqlstate: db_error.code().code().to_owned(),
            };
        }
        Self::new(PostgresErrorKind::Client, error.to_string())
    }

    #[must_use]
    pub fn pg_errno(&self) -> i64 {
        match self.kind {
            PostgresErrorKind::MissingDsn => 2002,
            PostgresErrorKind::InvalidQuery => 7,
            PostgresErrorKind::Client => 1,
        }
    }
}

impl fmt::Display for PostgresError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for PostgresError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostgresErrorKind {
    MissingDsn,
    InvalidQuery,
    Client,
}

enum PostgresParam {
    Null(Option<String>),
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

impl PostgresParam {
    fn as_tosql(&self) -> &(dyn ToSql + Sync) {
        match self {
            Self::Null(value) => value,
            Self::Bool(value) => value,
            Self::Int(value) => value,
            Self::Float(value) => value,
            Self::String(value) => value,
        }
    }
}

fn postgres_params(values: &[Value]) -> Result<Vec<PostgresParam>, PostgresError> {
    values
        .iter()
        .map(|value| {
            Ok(match value {
                Value::Null => PostgresParam::Null(None),
                Value::Bool(value) => PostgresParam::Bool(*value),
                Value::Int(value) => PostgresParam::Int(*value),
                Value::Float(value) => PostgresParam::Float(value.to_f64()),
                Value::String(value) => PostgresParam::String(value.to_string_lossy()),
                Value::Reference(cell) => {
                    return postgres_params(&[cell.get()]).map(|mut values| values.remove(0));
                }
                other => PostgresParam::String(
                    convert::to_string_php(other)
                        .map_or_else(|_| String::new(), |value| value.to_string_lossy()),
                ),
            })
        })
        .collect()
}

fn convert_row(row: Row) -> PostgresRow {
    let values = row
        .columns()
        .iter()
        .enumerate()
        .map(|(index, column)| cell_to_value(&row, index, column.type_()))
        .collect();
    PostgresRow { values }
}

fn cell_to_value(row: &Row, index: usize, ty: &Type) -> Value {
    if matches!(*ty, Type::INT2) {
        return row
            .try_get::<_, Option<i16>>(index)
            .ok()
            .flatten()
            .map_or(Value::Null, |value| Value::Int(i64::from(value)));
    }
    if matches!(*ty, Type::INT4) {
        return row
            .try_get::<_, Option<i32>>(index)
            .ok()
            .flatten()
            .map_or(Value::Null, |value| Value::Int(i64::from(value)));
    }
    if matches!(*ty, Type::INT8) {
        return row
            .try_get::<_, Option<i64>>(index)
            .ok()
            .flatten()
            .map_or(Value::Null, Value::Int);
    }
    if matches!(*ty, Type::BOOL) {
        return row
            .try_get::<_, Option<bool>>(index)
            .ok()
            .flatten()
            .map_or(Value::Null, Value::Bool);
    }
    if matches!(*ty, Type::FLOAT4 | Type::FLOAT8) {
        return row
            .try_get::<_, Option<f64>>(index)
            .ok()
            .flatten()
            .map_or(Value::Null, Value::float);
    }
    if matches!(*ty, Type::BYTEA) {
        return row
            .try_get::<_, Option<Vec<u8>>>(index)
            .ok()
            .flatten()
            .map_or(Value::Null, Value::string);
    }
    row.try_get::<_, Option<String>>(index)
        .ok()
        .flatten()
        .map_or(Value::Null, Value::string)
}

fn row_to_array(columns: &[String], row: &PostgresRow, mode: i64) -> Value {
    let mut array = PhpArray::new();
    if mode & PGSQL_NUM != 0 {
        for (index, value) in row.values.iter().enumerate() {
            array.insert(ArrayKey::Int(index as i64), value.clone());
        }
    }
    if mode & PGSQL_ASSOC != 0 {
        for (name, value) in columns.iter().zip(row.values.iter()) {
            array.insert(
                ArrayKey::String(PhpString::from_bytes(name.as_bytes().to_vec())),
                value.clone(),
            );
        }
    }
    Value::Array(array)
}

fn postgres_query_returns_rows(query: &str) -> bool {
    let query = query.trim_start().to_ascii_lowercase();
    ["select", "show", "with", "values", "explain"]
        .iter()
        .any(|prefix| query.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_dsn() {
        let error = PostgresConnectOptions::from_dsn("")
            .expect_err("empty PostgreSQL DSNs should be rejected");
        assert_eq!(error.kind, PostgresErrorKind::MissingDsn);
    }

    #[test]
    fn parses_dsn_without_connecting() {
        let options = PostgresConnectOptions::from_dsn(
            "host=localhost port=5432 dbname=app user=app password=secret",
        )
        .expect("libpq-style DSN should parse");
        assert!(format!("{options:?}").contains("dbname=app"));
    }

    #[test]
    fn pdo_pgsql_connect_options_preserve_libpq_options() {
        let options = PostgresConnectOptions::from_dsn(
            "host=127.0.0.1 port=5433 dbname=app user=app password=secret sslmode=disable",
        )
        .expect("PDO PostgreSQL DSN should map to libpq options");
        let rendered = format!("{options:?}");
        assert!(rendered.contains("host=127.0.0.1"), "{rendered}");
        assert!(rendered.contains("port=5433"), "{rendered}");
        assert!(rendered.contains("dbname=app"), "{rendered}");
        assert!(rendered.contains("sslmode=disable"), "{rendered}");
    }
}
