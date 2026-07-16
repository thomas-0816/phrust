<?php
// oracle-probe: id=oracle-builtin-contract-function-asort-f41c019dc9 area=builtin_contract kind=function symbol=asort source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-asort-f41c019dc9 failure_category=builtin_contract
try {
    $result = \asort();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
