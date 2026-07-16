<?php
// oracle-probe: id=oracle-builtin-contract-function-join-7f1035f392 area=builtin_contract kind=function symbol=join source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-join-7f1035f392 failure_category=builtin_contract
try {
    $result = \join();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
