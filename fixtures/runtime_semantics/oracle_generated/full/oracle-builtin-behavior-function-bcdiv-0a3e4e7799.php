<?php
// oracle-probe: id=oracle-builtin-behavior-function-bcdiv-0a3e4e7799 area=builtin_behavior kind=function symbol=bcdiv source=ext/bcmath/bcmath.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-bcdiv-0a3e4e7799 failure_category=builtin_behavior requires_ref_extension=bcmath
try {
    $result = \bcdiv(num1: "", num2: "");
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
