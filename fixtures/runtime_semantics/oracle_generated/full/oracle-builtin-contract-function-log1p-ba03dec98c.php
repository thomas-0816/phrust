<?php
// oracle-probe: id=oracle-builtin-contract-function-log1p-ba03dec98c area=builtin_contract kind=function symbol=log1p source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-log1p-ba03dec98c failure_category=builtin_contract
try {
    $result = \log1p();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
