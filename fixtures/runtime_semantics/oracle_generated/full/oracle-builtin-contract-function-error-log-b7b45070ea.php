<?php
// oracle-probe: id=oracle-builtin-contract-function-error-log-b7b45070ea area=builtin_contract kind=function symbol=error_log source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-error-log-b7b45070ea failure_category=builtin_contract
try {
    $result = \error_log();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
