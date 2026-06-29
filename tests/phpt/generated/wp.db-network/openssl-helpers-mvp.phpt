--TEST--
wp.db-network: selected OpenSSL helper behavior
--DESCRIPTION--
coverage for digest, random bytes, method listing, and explicit
verification gap behavior.
--SKIPIF--
<?php
if (!extension_loaded("openssl")) {
    die("skip openssl extension is not loaded");
}
?>
--FILE--
<?php
$bytes = openssl_random_pseudo_bytes(8);
var_dump(strlen($bytes));
var_dump(openssl_digest("abc", "sha256"));
var_dump(in_array("sha256", openssl_get_md_methods(), true));
var_dump(openssl_verify("data", "signature", "public-key") === -1);
?>
--EXPECT--
int(8)
string(64) "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
bool(true)
bool(true)
