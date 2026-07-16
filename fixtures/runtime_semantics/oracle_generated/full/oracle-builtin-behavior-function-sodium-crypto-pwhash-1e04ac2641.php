<?php
// oracle-probe: id=oracle-builtin-behavior-function-sodium-crypto-pwhash-1e04ac2641 area=builtin_behavior kind=function symbol=sodium_crypto_pwhash source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-sodium-crypto-pwhash-1e04ac2641 failure_category=builtin_behavior requires_ref_extension=sodium
try {
    $result = \sodium_crypto_pwhash(length: 0, password: "", salt: "", opslimit: 0, memlimit: 0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
