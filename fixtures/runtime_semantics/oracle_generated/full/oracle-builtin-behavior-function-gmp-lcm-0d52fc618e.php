<?php
// oracle-probe: id=oracle-builtin-behavior-function-gmp-lcm-0d52fc618e area=builtin_behavior kind=function symbol=gmp_lcm source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-gmp-lcm-0d52fc618e failure_category=builtin_behavior requires_ref_extension=gmp
try {
    $result = \gmp_lcm(0, 0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
