<?php
// oracle-probe: id=oracle-builtin-contract-function-sodium-crypto-pwhash-scryptsalsa208sha256-str-916561da21 area=builtin_contract kind=function symbol=sodium_crypto_pwhash_scryptsalsa208sha256_str source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sodium-crypto-pwhash-scryptsalsa208sha256-str-916561da21 failure_category=builtin_contract requires_ref_extension=sodium
$name = "sodium_crypto_pwhash_scryptsalsa208sha256_str";
echo function_exists($name) ? "available\n" : "missing\n";
