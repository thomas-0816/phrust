<?php
// oracle-probe: id=oracle-builtin-behavior-function-sodium-crypto-secretbox-open-ddaeaa9fc0 area=builtin_behavior kind=function symbol=sodium_crypto_secretbox_open source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-sodium-crypto-secretbox-open-ddaeaa9fc0 failure_category=builtin_behavior requires_ref_extension=sodium
try {
    $result = \sodium_crypto_secretbox_open("", "", "");
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
