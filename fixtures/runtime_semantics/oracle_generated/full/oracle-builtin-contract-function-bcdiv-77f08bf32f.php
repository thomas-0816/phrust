<?php
// oracle-probe: id=oracle-builtin-contract-function-bcdiv-77f08bf32f area=builtin_contract kind=function symbol=bcdiv source=ext/bcmath/bcmath.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-bcdiv-77f08bf32f failure_category=builtin_contract requires_ref_extension=bcmath
try {
    $result = \bcdiv();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
