<?php
// oracle-probe: id=oracle-builtin-contract-function-debug-print-backtrace-a9eac4ac81 area=builtin_contract kind=function symbol=debug_print_backtrace source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-debug-print-backtrace-a9eac4ac81 failure_category=builtin_contract
try {
    $result = \debug_print_backtrace(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
