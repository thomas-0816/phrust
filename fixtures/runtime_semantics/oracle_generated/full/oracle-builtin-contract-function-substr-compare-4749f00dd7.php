<?php
// oracle-probe: id=oracle-builtin-contract-function-substr-compare-4749f00dd7 area=builtin_contract kind=function symbol=substr_compare source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-substr-compare-4749f00dd7 failure_category=builtin_contract
try {
    $result = \substr_compare();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
