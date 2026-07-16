<?php
// oracle-probe: id=oracle-builtin-contract-function-fileperms-6143983114 area=builtin_contract kind=function symbol=fileperms source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-fileperms-6143983114 failure_category=builtin_contract
try {
    $result = \fileperms();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
