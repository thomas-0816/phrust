<?php
// oracle-probe: id=oracle-builtin-contract-function-get-loaded-extensions-ab5ef1d8da area=builtin_contract kind=function symbol=get_loaded_extensions source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-loaded-extensions-ab5ef1d8da failure_category=builtin_contract
try {
    $result = \get_loaded_extensions(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
