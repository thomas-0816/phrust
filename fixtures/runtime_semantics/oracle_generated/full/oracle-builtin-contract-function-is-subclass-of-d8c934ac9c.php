<?php
// oracle-probe: id=oracle-builtin-contract-function-is-subclass-of-d8c934ac9c area=builtin_contract kind=function symbol=is_subclass_of source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-subclass-of-d8c934ac9c failure_category=builtin_contract
try {
    $result = \is_subclass_of();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
