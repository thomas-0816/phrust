<?php
// oracle-probe: id=oracle-builtin-contract-function-array-key-last-0043fbc55c area=builtin_contract kind=function symbol=array_key_last source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-key-last-0043fbc55c failure_category=builtin_contract
try {
    $result = \array_key_last();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
