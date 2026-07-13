--TEST--
sodium libsodium-backed secretbox, box, AEAD, KDF, and pwhash
--EXTENSIONS--
sodium
--FILE--
<?php
$nonce = str_repeat("\x01", SODIUM_CRYPTO_SECRETBOX_NONCEBYTES);
$key = str_repeat("\x02", SODIUM_CRYPTO_SECRETBOX_KEYBYTES);
$ciphertext = sodium_crypto_secretbox("test", $nonce, $key);
var_dump(strlen($ciphertext));
var_dump(sodium_crypto_secretbox_open($ciphertext, $nonce, $key));
var_dump(sodium_crypto_secretbox_open("\x00" . $ciphertext, $nonce, $key));
try {
    sodium_crypto_secretbox("test", substr($nonce, 1), $key);
} catch (SodiumException $e) {
    echo "secretbox nonce\n";
}

$seed = str_repeat("\x07", SODIUM_CRYPTO_BOX_SEEDBYTES);
$keypair = sodium_crypto_box_seed_keypair($seed);
$secretKey = sodium_crypto_box_secretkey($keypair);
$publicKey = sodium_crypto_box_publickey($keypair);
var_dump(strlen($keypair));
var_dump(strlen($secretKey));
var_dump(strlen($publicKey));
var_dump(sodium_crypto_box_publickey_from_secretkey($secretKey) === $publicKey);
$rebuiltKeypair = sodium_crypto_box_keypair_from_secretkey_and_publickey($secretKey, $publicKey);
var_dump($rebuiltKeypair === $keypair);
$boxNonce = str_repeat("\x08", SODIUM_CRYPTO_BOX_NONCEBYTES);
$boxCiphertext = sodium_crypto_box("box", $boxNonce, $rebuiltKeypair);
var_dump(strlen($boxCiphertext));
var_dump(sodium_crypto_box_open($boxCiphertext, $boxNonce, $rebuiltKeypair));
$sealed = sodium_crypto_box_seal("sealed", $publicKey);
var_dump(strlen($sealed));
var_dump(sodium_crypto_box_seal_open($sealed, $keypair));
try {
    sodium_crypto_box("box", substr($boxNonce, 1), $keypair);
} catch (SodiumException $e) {
    echo "box nonce\n";
}

$aeadKey = str_repeat("\x03", SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_KEYBYTES);
$aeadNonce = str_repeat("\x04", SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_NPUBBYTES);
$aeadCiphertext = sodium_crypto_aead_xchacha20poly1305_ietf_encrypt("msg", "ad", $aeadNonce, $aeadKey);
var_dump(strlen($aeadCiphertext));
var_dump(sodium_crypto_aead_xchacha20poly1305_ietf_decrypt($aeadCiphertext, "ad", $aeadNonce, $aeadKey));
var_dump(sodium_crypto_aead_xchacha20poly1305_ietf_decrypt($aeadCiphertext, "bad", $aeadNonce, $aeadKey));
try {
    sodium_crypto_aead_xchacha20poly1305_ietf_decrypt($aeadCiphertext, "ad", $aeadKey, $aeadNonce);
} catch (SodiumException $e) {
    echo "aead lengths\n";
}

$kdfKey = str_repeat("\x05", SODIUM_CRYPTO_KDF_KEYBYTES);
$subkey1 = sodium_crypto_kdf_derive_from_key(SODIUM_CRYPTO_KDF_BYTES_MIN, 0, "context!", $kdfKey);
$subkey2 = sodium_crypto_kdf_derive_from_key(SODIUM_CRYPTO_KDF_BYTES_MIN, 1, "context!", $kdfKey);
$subkey3 = sodium_crypto_kdf_derive_from_key(SODIUM_CRYPTO_KDF_BYTES_MIN, 0, "context!", $kdfKey);
var_dump(strlen($subkey1));
var_dump($subkey1 !== $subkey2);
var_dump($subkey1 === $subkey3);
try {
    sodium_crypto_kdf_derive_from_key(10, 0, "context!", $kdfKey);
} catch (SodiumException $e) {
    echo "kdf length\n";
}

$salt = str_repeat("\x06", SODIUM_CRYPTO_PWHASH_SALTBYTES);
$rawHash = sodium_crypto_pwhash(16, "password", $salt, SODIUM_CRYPTO_PWHASH_OPSLIMIT_INTERACTIVE, SODIUM_CRYPTO_PWHASH_MEMLIMIT_INTERACTIVE);
var_dump(strlen($rawHash));
$hash = sodium_crypto_pwhash_str("password", SODIUM_CRYPTO_PWHASH_OPSLIMIT_INTERACTIVE, SODIUM_CRYPTO_PWHASH_MEMLIMIT_INTERACTIVE);
var_dump(strpos($hash, SODIUM_CRYPTO_PWHASH_STRPREFIX) === 0);
var_dump(sodium_crypto_pwhash_str_verify($hash, "password"));
var_dump(sodium_crypto_pwhash_str_verify($hash, "wrong"));
var_dump(sodium_crypto_pwhash_str_needs_rehash($hash, SODIUM_CRYPTO_PWHASH_OPSLIMIT_INTERACTIVE, SODIUM_CRYPTO_PWHASH_MEMLIMIT_INTERACTIVE));

$scryptSalt = str_repeat("\x09", SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_SALTBYTES);
$scryptRaw = sodium_crypto_pwhash_scryptsalsa208sha256(16, "password", $scryptSalt, SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_OPSLIMIT_INTERACTIVE, SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_MEMLIMIT_INTERACTIVE);
var_dump(strlen($scryptRaw));
$scryptHash = sodium_crypto_pwhash_scryptsalsa208sha256_str("password", SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_OPSLIMIT_INTERACTIVE, SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_MEMLIMIT_INTERACTIVE);
var_dump(strpos($scryptHash, SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_STRPREFIX) === 0);
var_dump(sodium_crypto_pwhash_scryptsalsa208sha256_str_verify($scryptHash, "password"));
var_dump(sodium_crypto_pwhash_scryptsalsa208sha256_str_verify($scryptHash, "wrong"));
?>
--EXPECT--
int(20)
string(4) "test"
bool(false)
secretbox nonce
int(64)
int(32)
int(32)
bool(true)
bool(true)
int(19)
string(3) "box"
int(54)
string(6) "sealed"
box nonce
int(19)
string(3) "msg"
bool(false)
aead lengths
int(16)
bool(true)
bool(true)
kdf length
int(16)
bool(true)
bool(true)
bool(false)
bool(false)
int(16)
bool(true)
bool(true)
bool(false)
