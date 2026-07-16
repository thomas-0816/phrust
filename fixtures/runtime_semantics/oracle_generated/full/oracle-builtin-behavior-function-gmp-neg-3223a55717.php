<?php
// oracle-probe: id=oracle-builtin-behavior-function-gmp-neg-3223a55717 area=builtin_behavior kind=function symbol=gmp_neg source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-gmp-neg-3223a55717 failure_category=builtin_behavior requires_ref_extension=gmp
try {
    $result = \gmp_neg([]);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
