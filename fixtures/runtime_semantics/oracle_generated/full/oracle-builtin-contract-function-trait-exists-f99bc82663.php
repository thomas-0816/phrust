<?php
// oracle-probe: id=oracle-builtin-contract-function-trait-exists-f99bc82663 area=builtin_contract kind=function symbol=trait_exists source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-trait-exists-f99bc82663 failure_category=builtin_contract
try {
    $result = \trait_exists();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
