<?php
// oracle-probe: id=oracle-builtin-contract-function-log10-120dd9fe3e area=builtin_contract kind=function symbol=log10 source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-log10-120dd9fe3e failure_category=builtin_contract
try {
    $result = \log10();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
