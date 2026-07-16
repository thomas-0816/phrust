<?php
// oracle-probe: id=oracle-builtin-contract-function-define-9eb59426d0 area=builtin_contract kind=function symbol=define source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-define-9eb59426d0 failure_category=builtin_contract
try {
    $result = \define();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
