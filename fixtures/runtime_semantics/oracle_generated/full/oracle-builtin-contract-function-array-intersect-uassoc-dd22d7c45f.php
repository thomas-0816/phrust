<?php
// oracle-probe: id=oracle-builtin-contract-function-array-intersect-uassoc-dd22d7c45f area=builtin_contract kind=function symbol=array_intersect_uassoc source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-intersect-uassoc-dd22d7c45f failure_category=builtin_contract
try {
    $result = \array_intersect_uassoc();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
