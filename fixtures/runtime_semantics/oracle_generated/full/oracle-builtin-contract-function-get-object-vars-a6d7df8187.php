<?php
// oracle-probe: id=oracle-builtin-contract-function-get-object-vars-a6d7df8187 area=builtin_contract kind=function symbol=get_object_vars source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-object-vars-a6d7df8187 failure_category=builtin_contract
try {
    $result = \get_object_vars();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
