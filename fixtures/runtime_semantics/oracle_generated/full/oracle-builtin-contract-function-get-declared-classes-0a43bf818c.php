<?php
// oracle-probe: id=oracle-builtin-contract-function-get-declared-classes-0a43bf818c area=builtin_contract kind=function symbol=get_declared_classes source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-declared-classes-0a43bf818c failure_category=builtin_contract
try {
    $result = \get_declared_classes(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
