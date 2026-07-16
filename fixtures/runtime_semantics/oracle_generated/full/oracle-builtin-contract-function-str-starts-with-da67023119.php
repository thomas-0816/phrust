<?php
// oracle-probe: id=oracle-builtin-contract-function-str-starts-with-da67023119 area=builtin_contract kind=function symbol=str_starts_with source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-str-starts-with-da67023119 failure_category=builtin_contract
try {
    $result = \str_starts_with();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
