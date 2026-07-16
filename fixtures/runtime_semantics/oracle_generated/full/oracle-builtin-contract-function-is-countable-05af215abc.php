<?php
// oracle-probe: id=oracle-builtin-contract-function-is-countable-05af215abc area=builtin_contract kind=function symbol=is_countable source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-countable-05af215abc failure_category=builtin_contract
try {
    $result = \is_countable();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
