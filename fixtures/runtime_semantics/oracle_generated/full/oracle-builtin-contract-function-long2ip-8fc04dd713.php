<?php
// oracle-probe: id=oracle-builtin-contract-function-long2ip-8fc04dd713 area=builtin_contract kind=function symbol=long2ip source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-long2ip-8fc04dd713 failure_category=builtin_contract
try {
    $result = \long2ip();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
