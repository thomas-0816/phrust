<?php
// oracle-probe: id=oracle-builtin-contract-function-proc-get-status-635be186dd area=builtin_contract kind=function symbol=proc_get_status source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-proc-get-status-635be186dd failure_category=builtin_contract
try {
    $result = \proc_get_status();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
