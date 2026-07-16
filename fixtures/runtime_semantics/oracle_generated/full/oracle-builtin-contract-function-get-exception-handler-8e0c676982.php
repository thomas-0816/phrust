<?php
// oracle-probe: id=oracle-builtin-contract-function-get-exception-handler-8e0c676982 area=builtin_contract kind=function symbol=get_exception_handler source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-exception-handler-8e0c676982 failure_category=builtin_contract
try {
    $result = \get_exception_handler(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
