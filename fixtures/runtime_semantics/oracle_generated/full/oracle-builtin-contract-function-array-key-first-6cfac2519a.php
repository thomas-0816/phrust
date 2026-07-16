<?php
// oracle-probe: id=oracle-builtin-contract-function-array-key-first-6cfac2519a area=builtin_contract kind=function symbol=array_key_first source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-key-first-6cfac2519a failure_category=builtin_contract
try {
    $result = \array_key_first();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
