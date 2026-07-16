<?php
// oracle-probe: id=oracle-builtin-contract-function-strlen-b8007d5975 area=builtin_contract kind=function symbol=strlen source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-strlen-b8007d5975 failure_category=builtin_contract
try {
    $result = \strlen();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
