<?php
// oracle-probe: id=oracle-builtin-contract-function-sha1-7a06b30bab area=builtin_contract kind=function symbol=sha1 source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-sha1-7a06b30bab failure_category=builtin_contract
try {
    $result = \sha1();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
