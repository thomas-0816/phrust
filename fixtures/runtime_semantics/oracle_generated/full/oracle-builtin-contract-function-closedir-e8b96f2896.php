<?php
// oracle-probe: id=oracle-builtin-contract-function-closedir-e8b96f2896 area=builtin_contract kind=function symbol=closedir source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-closedir-e8b96f2896 failure_category=builtin_contract
try {
    $result = \closedir(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
