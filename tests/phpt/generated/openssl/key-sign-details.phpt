--TEST--
openssl: generated key export, details, sign and verify
--DESCRIPTION--
Coverage for app-facing OpenSSL key APIs using short-lived generated keys.
--SKIPIF--
<?php
if (!extension_loaded("openssl")) {
    die("skip openssl extension is not loaded");
}
?>
--FILE--
<?php
$key = openssl_pkey_new(["private_key_bits" => 1024]);
var_dump(is_string($key));

$exported = null;
var_dump(openssl_pkey_export($key, $exported));
var_dump(is_string($exported));

$public = openssl_pkey_get_public($key);
var_dump(is_string($public));

$private = openssl_pkey_get_private($exported);
var_dump(is_string($private));

$details = openssl_pkey_get_details($key);
var_dump(is_array($details));
var_dump($details["type"] === OPENSSL_KEYTYPE_RSA);
var_dump($details["bits"] >= 1024);
var_dump(is_string($details["key"]));

$signature = null;
var_dump(openssl_sign("payload", $signature, $key, OPENSSL_ALGO_SHA256));
var_dump(is_string($signature));
var_dump(openssl_verify("payload", $signature, $public, OPENSSL_ALGO_SHA256));
var_dump(openssl_verify("changed", $signature, $public, OPENSSL_ALGO_SHA256));
var_dump(openssl_error_string());
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
bool(true)
bool(true)
int(1)
int(0)
bool(false)
