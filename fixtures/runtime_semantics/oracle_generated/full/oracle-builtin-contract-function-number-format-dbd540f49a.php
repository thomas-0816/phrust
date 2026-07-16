<?php
// oracle-probe: id=oracle-builtin-contract-function-number-format-dbd540f49a area=builtin_contract kind=function symbol=number_format source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-number-format-dbd540f49a failure_category=builtin_contract
try {
    $result = \number_format();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
