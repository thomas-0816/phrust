<?php
// oracle-probe: id=oracle-builtin-contract-function-gmp-random-range-2447178276 area=builtin_contract kind=function symbol=gmp_random_range source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gmp-random-range-2447178276 failure_category=builtin_contract requires_ref_extension=gmp
try {
    $result = \gmp_random_range();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
