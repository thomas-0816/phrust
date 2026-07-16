<?php
// oracle-probe: id=oracle-builtin-contract-function-hash-hmac-9012d6f366 area=builtin_contract kind=function symbol=hash_hmac source=ext/hash/hash.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-hash-hmac-9012d6f366 failure_category=builtin_contract requires_ref_extension=hash
try {
    $result = \hash_hmac();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
