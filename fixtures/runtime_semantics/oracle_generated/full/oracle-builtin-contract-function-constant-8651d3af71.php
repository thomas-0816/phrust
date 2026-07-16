<?php
// oracle-probe: id=oracle-builtin-contract-function-constant-8651d3af71 area=builtin_contract kind=function symbol=constant source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-constant-8651d3af71 failure_category=builtin_contract
try {
    $result = \constant();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
