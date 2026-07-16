<?php
// oracle-probe: id=oracle-builtin-contract-function-array-replace-recursive-d4d1980c57 area=builtin_contract kind=function symbol=array_replace_recursive source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-replace-recursive-d4d1980c57 failure_category=builtin_contract
try {
    $result = \array_replace_recursive();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
