<?php
// oracle-probe: id=oracle-builtin-contract-function-is-bool-6a7b0d9b47 area=builtin_contract kind=function symbol=is_bool source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-bool-6a7b0d9b47 failure_category=builtin_contract
try {
    $result = \is_bool();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
