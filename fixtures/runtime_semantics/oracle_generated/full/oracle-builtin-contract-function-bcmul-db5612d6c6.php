<?php
// oracle-probe: id=oracle-builtin-contract-function-bcmul-db5612d6c6 area=builtin_contract kind=function symbol=bcmul source=ext/bcmath/bcmath.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-bcmul-db5612d6c6 failure_category=builtin_contract requires_ref_extension=bcmath
try {
    $result = \bcmul();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
