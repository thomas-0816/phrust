<?php
// oracle-probe: id=oracle-builtin-contract-function-defined-e4b3967f27 area=builtin_contract kind=function symbol=defined source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-defined-e4b3967f27 failure_category=builtin_contract
try {
    $result = \defined();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
