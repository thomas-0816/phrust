<?php
// oracle-probe: id=oracle-builtin-contract-function-str-replace-14dd64a547 area=builtin_contract kind=function symbol=str_replace source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-str-replace-14dd64a547 failure_category=builtin_contract
try {
    $result = \str_replace();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
