<?php
// oracle-probe: id=oracle-builtin-contract-function-class-exists-205c7e9833 area=builtin_contract kind=function symbol=class_exists source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-class-exists-205c7e9833 failure_category=builtin_contract
try {
    $result = \class_exists();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
