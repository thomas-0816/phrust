--TEST--
wp.db-network: selected OpenSSL helpers are visible
--DESCRIPTION--
Prompt 3.7 coverage for HTTPS/security/update helper startup.
--SKIPIF--
<?php
if (!extension_loaded("openssl")) {
    die("skip openssl extension is not loaded");
}
?>
--FILE--
<?php
var_dump(extension_loaded("openssl"));
var_dump(function_exists("openssl_random_pseudo_bytes"));
var_dump(function_exists("openssl_digest"));
var_dump(function_exists("openssl_verify"));
var_dump(function_exists("openssl_get_md_methods"));
var_dump(defined("OPENSSL_ALGO_SHA256"));
?>
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
