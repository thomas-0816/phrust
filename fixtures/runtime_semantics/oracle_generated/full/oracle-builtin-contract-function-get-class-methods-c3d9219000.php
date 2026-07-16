<?php
// oracle-probe: id=oracle-builtin-contract-function-get-class-methods-c3d9219000 area=builtin_contract kind=function symbol=get_class_methods source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-class-methods-c3d9219000 failure_category=builtin_contract
try {
    $result = \get_class_methods();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
