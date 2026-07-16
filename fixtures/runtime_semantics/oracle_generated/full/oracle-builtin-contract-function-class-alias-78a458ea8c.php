<?php
// oracle-probe: id=oracle-builtin-contract-function-class-alias-78a458ea8c area=builtin_contract kind=function symbol=class_alias source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-class-alias-78a458ea8c failure_category=builtin_contract
try {
    $result = \class_alias();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
