<?php
// oracle-probe: id=oracle-builtin-contract-function-password-verify-6fb4d55fa2 area=builtin_contract kind=function symbol=password_verify source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-password-verify-6fb4d55fa2 failure_category=builtin_contract
try {
    $result = \password_verify();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
