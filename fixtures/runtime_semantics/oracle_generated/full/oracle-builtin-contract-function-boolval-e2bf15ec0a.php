<?php
// oracle-probe: id=oracle-builtin-contract-function-boolval-e2bf15ec0a area=builtin_contract kind=function symbol=boolval source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-boolval-e2bf15ec0a failure_category=builtin_contract
try {
    $result = \boolval();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
