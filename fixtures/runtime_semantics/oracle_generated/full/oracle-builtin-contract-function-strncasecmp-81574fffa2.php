<?php
// oracle-probe: id=oracle-builtin-contract-function-strncasecmp-81574fffa2 area=builtin_contract kind=function symbol=strncasecmp source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-strncasecmp-81574fffa2 failure_category=builtin_contract
try {
    $result = \strncasecmp();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
