<?php
// oracle-probe: id=oracle-builtin-contract-function-interface-exists-7b04dff203 area=builtin_contract kind=function symbol=interface_exists source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-interface-exists-7b04dff203 failure_category=builtin_contract
try {
    $result = \interface_exists();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
