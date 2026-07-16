<?php
// oracle-probe: id=oracle-builtin-contract-function-is-iterable-e4704259d6 area=builtin_contract kind=function symbol=is_iterable source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-iterable-e4704259d6 failure_category=builtin_contract
try {
    $result = \is_iterable();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
