<?php
// oracle-probe: id=oracle-builtin-contract-function-ip2long-db82638d4b area=builtin_contract kind=function symbol=ip2long source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-ip2long-db82638d4b failure_category=builtin_contract
try {
    $result = \ip2long();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
