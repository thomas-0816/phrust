<?php
// oracle-probe: id=oracle-builtin-contract-function-substr-count-0e0cd2e75b area=builtin_contract kind=function symbol=substr_count source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-substr-count-0e0cd2e75b failure_category=builtin_contract
try {
    $result = \substr_count();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
