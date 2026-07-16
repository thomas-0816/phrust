<?php
// oracle-probe: id=oracle-builtin-behavior-function-gmp-random-range-1c9e1ea587 area=builtin_behavior kind=function symbol=gmp_random_range source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-gmp-random-range-1c9e1ea587 failure_category=builtin_behavior requires_ref_extension=gmp
try {
    $result = \gmp_random_range(min: 0, max: 0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
