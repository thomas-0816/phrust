<?php
// oracle-probe: id=oracle-builtin-behavior-function-gmp-popcount-0db9d6e26a area=builtin_behavior kind=function symbol=gmp_popcount source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-gmp-popcount-0db9d6e26a failure_category=builtin_behavior requires_ref_extension=gmp
try {
    $result = \gmp_popcount([]);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
