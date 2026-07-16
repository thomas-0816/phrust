<?php
// oracle-probe: id=oracle-builtin-contract-function-sodium-bin2base64-fdb49dd9f3 area=builtin_contract kind=function symbol=sodium_bin2base64 source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sodium-bin2base64-fdb49dd9f3 failure_category=builtin_contract requires_ref_extension=sodium
try {
    $result = \sodium_bin2base64();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
