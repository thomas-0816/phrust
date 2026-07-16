<?php
// oracle-probe: id=oracle-builtin-behavior-function-bcsub-7d2b3285c1 area=builtin_behavior kind=function symbol=bcsub source=ext/bcmath/bcmath.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-bcsub-7d2b3285c1 failure_category=builtin_behavior requires_ref_extension=bcmath
try {
    $result = \bcsub([], "");
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
