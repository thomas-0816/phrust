<?php
// oracle-probe: id=oracle-builtin-contract-function-intval-42b2d7ffbe area=builtin_contract kind=function symbol=intval source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-intval-42b2d7ffbe failure_category=builtin_contract
try {
    $result = \intval();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
