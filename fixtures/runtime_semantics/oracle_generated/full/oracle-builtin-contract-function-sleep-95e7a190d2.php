<?php
// oracle-probe: id=oracle-builtin-contract-function-sleep-95e7a190d2 area=builtin_contract kind=function symbol=sleep source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-sleep-95e7a190d2 failure_category=builtin_contract
try {
    $result = \sleep();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
