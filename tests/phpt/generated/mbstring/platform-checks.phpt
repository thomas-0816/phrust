--TEST--
mbstring: platform checks stay negative for stub strategy
--DESCRIPTION--
Focused mbstring stub coverage for Composer-style platform checks.
--FILE--
<?php
var_dump(extension_loaded("mbstring"));
var_dump(function_exists("mb_strlen"));
var_dump(function_exists("mb_substr"));
var_dump(function_exists("mb_strtolower"));
var_dump(function_exists("mb_strtoupper"));
var_dump(function_exists("mb_detect_encoding"));
?>
--EXPECT--
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
bool(false)
