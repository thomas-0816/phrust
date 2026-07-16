<?php
// oracle-probe: id=oracle-builtin-contract-function-mhash-count-8679611299 area=builtin_contract kind=function symbol=mhash_count source=ext/hash/hash.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mhash-count-8679611299 failure_category=builtin_contract requires_ref_extension=hash
try {
    $result = \mhash_count(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
