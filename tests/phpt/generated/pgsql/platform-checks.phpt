--TEST--
pgsql: platform checks expose procedural PostgreSQL surface
--DESCRIPTION--
Generated coverage for the bounded procedural pgsql platform surface:
extension visibility, generated PgSql classes, constants, and high-use
function registration.
--EXTENSIONS--
pgsql
--FILE--
<?php
var_dump(extension_loaded("pgsql"));
var_dump(class_exists("PgSql\\Connection", false));
var_dump(class_exists("PgSql\\Result", false));
var_dump(class_exists("PgSql\\Lob", false));
var_dump(PGSQL_ASSOC);
var_dump(PGSQL_NUM);
var_dump(PGSQL_BOTH);
var_dump(PGSQL_CONNECTION_OK);
foreach ([
    "pg_connect",
    "pg_pconnect",
    "pg_close",
    "pg_query",
    "pg_query_params",
    "pg_prepare",
    "pg_execute",
    "pg_fetch_array",
    "pg_fetch_assoc",
    "pg_fetch_row",
    "pg_fetch_object",
    "pg_fetch_result",
    "pg_free_result",
    "pg_num_rows",
    "pg_num_fields",
    "pg_affected_rows",
    "pg_last_error",
    "pg_result_error",
    "pg_escape_string",
    "pg_escape_literal",
    "pg_escape_identifier",
    "pg_escape_bytea",
] as $function) {
    var_dump(function_exists($function));
}
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
int(1)
int(2)
int(3)
int(0)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
