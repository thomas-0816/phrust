<?php
// oracle-probe: id=oracle-builtin-contract-function-shell-exec-207644822b area=builtin_contract kind=function symbol=shell_exec source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-shell-exec-207644822b failure_category=builtin_contract
try {
    $result = \shell_exec();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
