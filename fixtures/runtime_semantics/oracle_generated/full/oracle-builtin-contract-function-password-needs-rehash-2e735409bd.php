<?php
// oracle-probe: id=oracle-builtin-contract-function-password-needs-rehash-2e735409bd area=builtin_contract kind=function symbol=password_needs_rehash source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-password-needs-rehash-2e735409bd failure_category=builtin_contract
try {
    $result = \password_needs_rehash();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
