--TEST--
wp.db-network: curl MVP platform visibility
--DESCRIPTION--
Prompt 3.6 coverage for WordPress remote request transport startup.
--SKIPIF--
<?php
if (!extension_loaded("curl")) {
    die("skip curl extension is not loaded");
}
?>
--FILE--
<?php
var_dump(extension_loaded("curl"));
var_dump(class_exists("CurlHandle", false));
var_dump(function_exists("curl_version"));
var_dump(function_exists("curl_init"));
var_dump(function_exists("curl_setopt"));
var_dump(function_exists("curl_exec"));
var_dump(function_exists("curl_getinfo"));
var_dump(defined("CURLOPT_RETURNTRANSFER"));
var_dump(defined("CURLINFO_RESPONSE_CODE"));
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
