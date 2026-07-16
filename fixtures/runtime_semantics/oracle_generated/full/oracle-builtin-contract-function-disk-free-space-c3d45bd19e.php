<?php
// oracle-probe: id=oracle-builtin-contract-function-disk-free-space-c3d45bd19e area=builtin_contract kind=function symbol=disk_free_space source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-disk-free-space-c3d45bd19e failure_category=builtin_contract
try {
    $result = \disk_free_space();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
