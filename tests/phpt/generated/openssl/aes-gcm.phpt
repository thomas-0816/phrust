--TEST--
openssl: AES-GCM encrypt/decrypt tag handling
--DESCRIPTION--
Coverage for AEAD cipher handling with tag output and AAD.
--SKIPIF--
<?php
if (!extension_loaded("openssl")) {
    die("skip openssl extension is not loaded");
}
?>
--FILE--
<?php
$key = "0123456789abcdef";
$iv = "123456789012";
$tag = null;
$ciphertext = openssl_encrypt("secret", "aes-128-gcm", $key, OPENSSL_RAW_DATA, $iv, $tag, "aad", 12);
var_dump(is_string($ciphertext));
var_dump(strlen($tag));
var_dump(openssl_decrypt($ciphertext, "aes-128-gcm", $key, OPENSSL_RAW_DATA, $iv, $tag, "aad"));
var_dump(openssl_decrypt($ciphertext, "aes-128-gcm", $key, OPENSSL_RAW_DATA, $iv, str_repeat("\0", 12), "aad"));
var_dump(in_array("aes-128-gcm", openssl_get_cipher_methods(), true));
var_dump(openssl_cipher_iv_length("aes-128-gcm"));
var_dump(openssl_cipher_key_length("aes-128-gcm"));
?>
--EXPECT--
bool(true)
int(12)
string(6) "secret"
bool(false)
bool(true)
int(12)
int(16)
