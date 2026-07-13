--TEST--
sodium libsodium-backed utilities, constants, and keygens
--EXTENSIONS--
sodium
--FILE--
<?php
$a = "test";
sodium_memzero($a);
var_dump($a);

$v = "\xFF\xFF\x80\x01\x02\x03\x04\x05\x06\x07\x08";
sodium_increment($v);
var_dump(bin2hex($v));

$w = "\x01\x02\x03\x04\x05\x06\x07\x08\xFA\xFB\xFC";
sodium_add($v, $w);
var_dump(bin2hex($v));

var_dump(sodium_memcmp("same", "same"));
var_dump(sodium_compare("\x01", "\x02"));

$padded = sodium_pad("xyz", 16);
var_dump(bin2hex($padded));
var_dump(sodium_unpad($padded, 16) === "xyz");

echo strlen(SODIUM_LIBRARY_VERSION) >= 5 ? "version\n" : "bad-version\n";
var_dump(SODIUM_LIBRARY_MAJOR_VERSION >= 4);
var_dump(SODIUM_CRYPTO_GENERICHASH_KEYBYTES);
var_dump(strlen(sodium_crypto_generichash_keygen()));
var_dump(strlen(sodium_crypto_secretbox_keygen()));
var_dump(strlen(sodium_crypto_auth_keygen()));
var_dump(strlen(sodium_crypto_shorthash_keygen()));
var_dump(strlen(sodium_crypto_kdf_keygen()));
var_dump(strlen(sodium_crypto_aead_xchacha20poly1305_ietf_keygen()));

try {
    sodium_increment(123);
} catch (SodiumException $e) {
    echo $e->getMessage(), "\n";
}
?>
--EXPECT--
NULL
string(22) "0000810102030405060708"
string(22) "0102840507090b0d000305"
int(0)
int(-1)
string(32) "78797a80000000000000000000000000"
bool(true)
version
bool(true)
int(32)
int(32)
int(32)
int(32)
int(16)
int(32)
int(32)
a PHP string is required
