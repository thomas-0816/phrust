<?php
// oracle-probe: id=oracle-builtin-contract-function-sodium-crypto-pwhash-str-needs-rehash-9ec511d91a area=builtin_contract kind=function symbol=sodium_crypto_pwhash_str_needs_rehash source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sodium-crypto-pwhash-str-needs-rehash-9ec511d91a failure_category=builtin_contract requires_ref_extension=sodium
try {
    $result = \sodium_crypto_pwhash_str_needs_rehash();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
