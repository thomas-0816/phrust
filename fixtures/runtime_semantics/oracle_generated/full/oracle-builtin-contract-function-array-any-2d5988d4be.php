<?php
// oracle-probe: id=oracle-builtin-contract-function-array-any-2d5988d4be area=builtin_contract kind=function symbol=array_any source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-any-2d5988d4be failure_category=builtin_contract
try {
    $result = \array_any();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
