<?php
// oracle-probe: id=oracle-builtin-contract-function-shuffle-345e3decf8 area=builtin_contract kind=function symbol=shuffle source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-shuffle-345e3decf8 failure_category=builtin_contract
try {
    $result = \shuffle();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
