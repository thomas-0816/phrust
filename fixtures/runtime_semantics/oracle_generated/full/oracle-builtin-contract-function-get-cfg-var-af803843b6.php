<?php
// oracle-probe: id=oracle-builtin-contract-function-get-cfg-var-af803843b6 area=builtin_contract kind=function symbol=get_cfg_var source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-cfg-var-af803843b6 failure_category=builtin_contract
try {
    $result = \get_cfg_var();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
