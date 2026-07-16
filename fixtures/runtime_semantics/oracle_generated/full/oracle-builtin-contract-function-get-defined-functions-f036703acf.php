<?php
// oracle-probe: id=oracle-builtin-contract-function-get-defined-functions-f036703acf area=builtin_contract kind=function symbol=get_defined_functions source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-defined-functions-f036703acf failure_category=builtin_contract
try {
    $result = \get_defined_functions(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
