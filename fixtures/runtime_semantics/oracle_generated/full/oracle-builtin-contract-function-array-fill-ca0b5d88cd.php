<?php
// oracle-probe: id=oracle-builtin-contract-function-array-fill-ca0b5d88cd area=builtin_contract kind=function symbol=array_fill source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-fill-ca0b5d88cd failure_category=builtin_contract
try {
    $result = \array_fill();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
