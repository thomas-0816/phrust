<?php
// oracle-probe: id=oracle-builtin-contract-function-get-current-user-2f9d1d6d5f area=builtin_contract kind=function symbol=get_current_user source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-current-user-2f9d1d6d5f failure_category=builtin_contract
try {
    $result = \get_current_user(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
