<?php
// oracle-probe: id=oracle-builtin-behavior-function-gmp-scan1-d424024091 area=builtin_behavior kind=function symbol=gmp_scan1 source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-gmp-scan1-d424024091 failure_category=builtin_behavior requires_ref_extension=gmp
try {
    $result = \gmp_scan1(num1: 0, start: 0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
