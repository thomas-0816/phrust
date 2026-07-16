<?php
// oracle-probe: id=oracle-builtin-contract-function-array-intersect-b8ca0c72d5 area=builtin_contract kind=function symbol=array_intersect source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-intersect-b8ca0c72d5 failure_category=builtin_contract
try {
    $result = \array_intersect();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
