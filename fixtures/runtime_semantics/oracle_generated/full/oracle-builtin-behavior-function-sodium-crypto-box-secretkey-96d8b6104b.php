<?php
// oracle-probe: id=oracle-builtin-behavior-function-sodium-crypto-box-secretkey-96d8b6104b area=builtin_behavior kind=function symbol=sodium_crypto_box_secretkey source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-sodium-crypto-box-secretkey-96d8b6104b failure_category=builtin_behavior requires_ref_extension=sodium
try {
    $result = \sodium_crypto_box_secretkey(key_pair: "");
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
