<?php
// oracle-probe: id=oracle-builtin-contract-function-get-resource-id-3036b9ed95 area=builtin_contract kind=function symbol=get_resource_id source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-resource-id-3036b9ed95 failure_category=builtin_contract
try {
    $result = \get_resource_id();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
