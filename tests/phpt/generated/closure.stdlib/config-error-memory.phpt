--TEST--
closure.stdlib: config, environment, error, and memory helpers
--DESCRIPTION--
Generated closure stdlib coverage for request-aware configuration,
environment, SAPI, error-reporting, memory, and time-limit helpers.
--FILE--
<?php
$previous_error_reporting = error_reporting(E_ERROR | E_WARNING);
var_dump(error_reporting());
error_reporting($previous_error_reporting);

$previous_charset = ini_set("default_charset", "UTF-8");
var_dump(is_string($previous_charset));
var_dump(ini_get("default_charset"));
$ini = ini_get_all(null, false);
var_dump(isset($ini["default_charset"]));
var_dump(get_cfg_var("__phrust_closure_missing_cfg_var__"));

putenv("PHRUST_CLOSURE_STDLIB_ENV=ok");
var_dump(getenv("PHRUST_CLOSURE_STDLIB_ENV"));
var_dump(is_string(php_sapi_name()));
var_dump(is_string(php_uname("s")) && php_uname("s") !== "");
var_dump(memory_get_usage() >= 0);
var_dump(memory_get_peak_usage() >= memory_get_usage());
var_dump(set_time_limit(2));
?>
--EXPECT--
int(3)
bool(true)
string(5) "UTF-8"
bool(true)
bool(false)
string(2) "ok"
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
