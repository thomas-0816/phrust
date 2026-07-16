<?php
// oracle-probe: id=oracle-builtin-behavior-function-gmp-gcd-6c94a56431 area=builtin_behavior kind=function symbol=gmp_gcd source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-gmp-gcd-6c94a56431 failure_category=builtin_behavior requires_ref_extension=gmp
try {
    $result = \gmp_gcd([], 0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
