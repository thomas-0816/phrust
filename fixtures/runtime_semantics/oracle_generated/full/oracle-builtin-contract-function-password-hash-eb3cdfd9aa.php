<?php
// oracle-probe: id=oracle-builtin-contract-function-password-hash-eb3cdfd9aa area=builtin_contract kind=function symbol=password_hash source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-password-hash-eb3cdfd9aa failure_category=builtin_contract
try {
    $result = \password_hash();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
