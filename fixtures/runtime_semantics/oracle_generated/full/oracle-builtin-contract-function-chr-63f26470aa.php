<?php
// oracle-probe: id=oracle-builtin-contract-function-chr-63f26470aa area=builtin_contract kind=function symbol=chr source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-chr-63f26470aa failure_category=builtin_contract
try {
    $result = \chr();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
