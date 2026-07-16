<?php
// oracle-probe: id=oracle-builtin-behavior-function-sodium-crypto-pwhash-scryptsalsa208sha256-str-verify-37bd596e77 area=builtin_behavior kind=function symbol=sodium_crypto_pwhash_scryptsalsa208sha256_str_verify source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-sodium-crypto-pwhash-scryptsalsa208sha256-str-verify-37bd596e77 failure_category=builtin_behavior requires_ref_extension=sodium
try {
    $result = \sodium_crypto_pwhash_scryptsalsa208sha256_str_verify([], "");
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
