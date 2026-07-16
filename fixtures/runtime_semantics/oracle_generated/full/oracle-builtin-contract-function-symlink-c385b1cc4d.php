<?php
// oracle-probe: id=oracle-builtin-contract-function-symlink-c385b1cc4d area=builtin_contract kind=function symbol=symlink source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-symlink-c385b1cc4d failure_category=builtin_contract
try {
    $result = \symlink();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
