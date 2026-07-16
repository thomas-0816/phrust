<?php
// oracle-probe: id=oracle-builtin-contract-function-fileowner-92987b8dd9 area=builtin_contract kind=function symbol=fileowner source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-fileowner-92987b8dd9 failure_category=builtin_contract
try {
    $result = \fileowner();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
