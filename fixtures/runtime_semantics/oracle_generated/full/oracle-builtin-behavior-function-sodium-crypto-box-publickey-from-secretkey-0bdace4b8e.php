<?php
// oracle-probe: id=oracle-builtin-behavior-function-sodium-crypto-box-publickey-from-secretkey-0bdace4b8e area=builtin_behavior kind=function symbol=sodium_crypto_box_publickey_from_secretkey source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-sodium-crypto-box-publickey-from-secretkey-0bdace4b8e failure_category=builtin_behavior requires_ref_extension=sodium
try {
    $result = \sodium_crypto_box_publickey_from_secretkey("");
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
