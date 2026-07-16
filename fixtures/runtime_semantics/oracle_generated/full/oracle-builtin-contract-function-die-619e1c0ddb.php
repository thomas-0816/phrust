<?php
// oracle-probe: id=oracle-builtin-contract-function-die-619e1c0ddb area=builtin_contract kind=function symbol=die source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-die-619e1c0ddb failure_category=builtin_contract
try {
    $result = \die(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
