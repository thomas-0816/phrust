<?php
// oracle-probe: id=oracle-builtin-contract-function-atan2-b7288908a9 area=builtin_contract kind=function symbol=atan2 source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-atan2-b7288908a9 failure_category=builtin_contract
try {
    $result = \atan2();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
