<?php
// oracle-probe: id=oracle-builtin-contract-function-ini-get-all-370a62bc5b area=builtin_contract kind=function symbol=ini_get_all source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-ini-get-all-370a62bc5b failure_category=builtin_contract
try {
    $result = \ini_get_all(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
