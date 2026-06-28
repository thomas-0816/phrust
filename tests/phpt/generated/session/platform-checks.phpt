--TEST--
session: platform checks stay negative
--DESCRIPTION--
Generated Branch 4 data-platform coverage for session classification without CLI session state.
--FILE--
<?php
var_dump(extension_loaded("session"));
var_dump(function_exists("session_start"));
var_dump(function_exists("session_id"));
var_dump(function_exists("session_status"));
var_dump(class_exists("SessionHandler", false));
?>
--EXPECT--
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
