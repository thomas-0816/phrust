<?php
// oracle-probe: id=oracle-builtin-contract-function-ini-set-152e39a223 area=builtin_contract kind=function symbol=ini_set source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-ini-set-152e39a223 failure_category=builtin_contract
try {
    $result = \ini_set();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
