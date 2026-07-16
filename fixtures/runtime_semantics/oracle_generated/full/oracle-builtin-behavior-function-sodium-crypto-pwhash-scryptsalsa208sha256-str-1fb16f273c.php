<?php
// oracle-probe: id=oracle-builtin-behavior-function-sodium-crypto-pwhash-scryptsalsa208sha256-str-1fb16f273c area=builtin_behavior kind=function symbol=sodium_crypto_pwhash_scryptsalsa208sha256_str source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-sodium-crypto-pwhash-scryptsalsa208sha256-str-1fb16f273c failure_category=builtin_behavior requires_ref_extension=sodium
try {
    $result = \sodium_crypto_pwhash_scryptsalsa208sha256_str([], 0, 0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
