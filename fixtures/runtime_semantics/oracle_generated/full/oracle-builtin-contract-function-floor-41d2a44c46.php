<?php
// oracle-probe: id=oracle-builtin-contract-function-floor-41d2a44c46 area=builtin_contract kind=function symbol=floor source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-floor-41d2a44c46 failure_category=builtin_contract
try {
    $result = \floor();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
