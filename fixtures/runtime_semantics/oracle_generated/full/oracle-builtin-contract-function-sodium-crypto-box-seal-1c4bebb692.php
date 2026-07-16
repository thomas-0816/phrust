<?php
// oracle-probe: id=oracle-builtin-contract-function-sodium-crypto-box-seal-1c4bebb692 area=builtin_contract kind=function symbol=sodium_crypto_box_seal source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sodium-crypto-box-seal-1c4bebb692 failure_category=builtin_contract requires_ref_extension=sodium
try {
    $result = \sodium_crypto_box_seal();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
