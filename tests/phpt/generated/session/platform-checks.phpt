--TEST--
session: CLI request-local state
--DESCRIPTION--
Generated coverage for deterministic CLI-only session functions and $_SESSION state.
--EXTENSIONS--
session
--FILE--
<?php
var_dump(extension_loaded("session"));
var_dump(function_exists("session_start"));
var_dump(function_exists("session_id"));
var_dump(function_exists("session_status"));
var_dump(class_exists("SessionHandler", false));
var_dump(PHP_SESSION_DISABLED, PHP_SESSION_NONE, PHP_SESSION_ACTIVE);
var_dump(session_status());
var_dump(session_name());
var_dump(session_name("LOCALID"));
var_dump(session_name());
var_dump(session_id("known"));
var_dump(session_id());
var_dump(session_start());
var_dump(session_status());
$_SESSION["alpha"] = "beta";
var_dump($_SESSION);
var_dump(session_destroy());
var_dump(session_status());
var_dump($_SESSION);
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(false)
int(0)
int(1)
int(2)
int(1)
string(9) "PHPSESSID"
string(9) "PHPSESSID"
string(7) "LOCALID"
string(0) ""
string(5) "known"
bool(true)
int(2)
array(1) {
  ["alpha"]=>
  string(4) "beta"
}
bool(true)
int(1)
array(0) {
}
