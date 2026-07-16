<?php
// oracle-probe: id=oracle-builtin-behavior-function-gmp-intval-72a7d24496 area=builtin_behavior kind=function symbol=gmp_intval source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-gmp-intval-72a7d24496 failure_category=builtin_behavior requires_ref_extension=gmp
try {
    $result = \gmp_intval(num: 0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
