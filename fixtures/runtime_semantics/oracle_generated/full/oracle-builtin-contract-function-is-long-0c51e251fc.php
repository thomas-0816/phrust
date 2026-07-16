<?php
// oracle-probe: id=oracle-builtin-contract-function-is-long-0c51e251fc area=builtin_contract kind=function symbol=is_long source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-long-0c51e251fc failure_category=builtin_contract
try {
    $result = \is_long();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
