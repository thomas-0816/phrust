<?php
// oracle-probe: id=oracle-builtin-contract-function-sodium-crypto-sign-verify-detached-7ac872c958 area=builtin_contract kind=function symbol=sodium_crypto_sign_verify_detached source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sodium-crypto-sign-verify-detached-7ac872c958 failure_category=builtin_contract requires_ref_extension=sodium
try {
    $result = \sodium_crypto_sign_verify_detached();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
