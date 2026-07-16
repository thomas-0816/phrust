<?php
// oracle-probe: id=oracle-builtin-contract-function-str-ends-with-0e6d606104 area=builtin_contract kind=function symbol=str_ends_with source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-str-ends-with-0e6d606104 failure_category=builtin_contract
try {
    $result = \str_ends_with();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
