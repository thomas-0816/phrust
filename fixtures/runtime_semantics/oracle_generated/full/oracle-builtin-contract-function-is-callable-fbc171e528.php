<?php
// oracle-probe: id=oracle-builtin-contract-function-is-callable-fbc171e528 area=builtin_contract kind=function symbol=is_callable source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-callable-fbc171e528 failure_category=builtin_contract
try {
    $result = \is_callable();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
