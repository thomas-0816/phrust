<?php
// oracle-probe: id=oracle-builtin-contract-function-acos-3ee3b980ac area=builtin_contract kind=function symbol=acos source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-acos-3ee3b980ac failure_category=builtin_contract
try {
    $result = \acos();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
