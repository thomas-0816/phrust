<?php
// oracle-probe: id=oracle-builtin-contract-function-forward-static-call-7d3d6bef7e area=builtin_contract kind=function symbol=forward_static_call source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-forward-static-call-7d3d6bef7e failure_category=builtin_contract
try {
    $result = \forward_static_call();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
