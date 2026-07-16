<?php
// oracle-probe: id=oracle-builtin-contract-function-natsort-03772aa636 area=builtin_contract kind=function symbol=natsort source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-natsort-03772aa636 failure_category=builtin_contract
try {
    $result = \natsort();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
