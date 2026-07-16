<?php
// oracle-probe: id=oracle-builtin-contract-function-getrandmax-13ea1ac1d5 area=builtin_contract kind=function symbol=getrandmax source=ext/random/random.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-getrandmax-13ea1ac1d5 failure_category=builtin_contract requires_ref_extension=random
try {
    $result = \getrandmax(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
