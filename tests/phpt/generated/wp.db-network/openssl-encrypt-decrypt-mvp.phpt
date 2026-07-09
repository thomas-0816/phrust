--TEST--
wp.db-network: selected OpenSSL AES-CBC encrypt/decrypt behavior
--DESCRIPTION--
coverage for deterministic AES-CBC helpers used by application crypto probes.
--SKIPIF--
<?php
if (!extension_loaded("openssl")) {
    die("skip openssl extension is not loaded");
}
?>
--FILE--
<?php
$data = "secret";
$key = "0123456789abcdef";
$iv = "1234567890abcdef";

$cipher = openssl_encrypt($data, "aes-128-cbc", $key, 0, $iv);
var_dump($cipher);
var_dump(openssl_decrypt($cipher, "aes-128-cbc", $key, 0, $iv));

$raw = openssl_encrypt($data, "aes-128-cbc", $key, OPENSSL_RAW_DATA, $iv);
var_dump(strlen($raw));
var_dump(openssl_decrypt($raw, "aes-128-cbc", $key, OPENSSL_RAW_DATA, $iv));

$padded = openssl_encrypt(str_repeat("x", 16), "aes-128-cbc", $key, OPENSSL_RAW_DATA | OPENSSL_ZERO_PADDING, $iv);
var_dump(strlen($padded));
var_dump(openssl_decrypt($padded, "aes-128-cbc", $key, OPENSSL_RAW_DATA | OPENSSL_ZERO_PADDING, $iv));

var_dump(openssl_error_string() === false);
var_dump(openssl_encrypt($data, "unknown-cipher", $key, 0, $iv));
var_dump(is_string(openssl_error_string()));
var_dump(openssl_error_string());
var_dump(openssl_encrypt($data, "aes-128-cbc", $key, OPENSSL_DONT_ZERO_PAD_KEY, $iv));
var_dump(is_string(openssl_error_string()));
var_dump(openssl_error_string());
?>
--EXPECTF--
string(24) "/romcUbbPYFPXuTCiUloyQ=="
string(6) "secret"
int(16)
string(6) "secret"
int(16)
string(16) "xxxxxxxxxxxxxxxx"
bool(true)

Warning: openssl_encrypt(): Unknown cipher algorithm in %s on line %d
bool(false)
bool(true)
bool(false)

Warning: openssl_encrypt(): Key length cannot be set for the cipher algorithm in %s on line %d
bool(false)
bool(true)
bool(false)
