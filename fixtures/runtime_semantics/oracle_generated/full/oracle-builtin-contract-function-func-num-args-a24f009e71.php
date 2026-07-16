<?php
// oracle-probe: id=oracle-builtin-contract-function-func-num-args-a24f009e71 area=builtin_contract kind=function symbol=func_num_args source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-func-num-args-a24f009e71 failure_category=builtin_contract
try {
    $result = \func_num_args(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
