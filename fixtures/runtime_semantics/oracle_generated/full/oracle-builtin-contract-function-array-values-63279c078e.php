<?php
// oracle-probe: id=oracle-builtin-contract-function-array-values-63279c078e area=builtin_contract kind=function symbol=array_values source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-values-63279c078e failure_category=builtin_contract
try {
    $result = \array_values();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
