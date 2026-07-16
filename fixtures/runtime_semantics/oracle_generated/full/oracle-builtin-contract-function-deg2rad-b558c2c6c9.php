<?php
// oracle-probe: id=oracle-builtin-contract-function-deg2rad-b558c2c6c9 area=builtin_contract kind=function symbol=deg2rad source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-deg2rad-b558c2c6c9 failure_category=builtin_contract
try {
    $result = \deg2rad();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
