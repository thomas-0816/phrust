<?php
// oracle-probe: id=oracle-builtin-contract-function-get-included-files-ea600cfe9a area=builtin_contract kind=function symbol=get_included_files source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-included-files-ea600cfe9a failure_category=builtin_contract
try {
    $result = \get_included_files(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
