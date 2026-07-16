<?php
// oracle-probe: id=oracle-builtin-contract-function-hash-hmac-algos-57be9ca23e area=builtin_contract kind=function symbol=hash_hmac_algos source=ext/hash/hash.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-hash-hmac-algos-57be9ca23e failure_category=builtin_contract requires_ref_extension=hash
try {
    $result = \hash_hmac_algos(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
