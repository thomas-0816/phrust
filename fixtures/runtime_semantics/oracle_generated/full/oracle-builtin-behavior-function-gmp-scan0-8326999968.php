<?php
// oracle-probe: id=oracle-builtin-behavior-function-gmp-scan0-8326999968 area=builtin_behavior kind=function symbol=gmp_scan0 source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-gmp-scan0-8326999968 failure_category=builtin_behavior requires_ref_extension=gmp
try {
    $result = \gmp_scan0(num1: 0, start: 0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
