<?php
// oracle-probe: id=oracle-builtin-contract-function-is-float-29f7e1fc0c area=builtin_contract kind=function symbol=is_float source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-float-29f7e1fc0c failure_category=builtin_contract
try {
    $result = \is_float();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
