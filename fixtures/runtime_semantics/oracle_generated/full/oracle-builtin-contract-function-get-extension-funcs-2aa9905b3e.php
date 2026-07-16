<?php
// oracle-probe: id=oracle-builtin-contract-function-get-extension-funcs-2aa9905b3e area=builtin_contract kind=function symbol=get_extension_funcs source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-extension-funcs-2aa9905b3e failure_category=builtin_contract
try {
    $result = \get_extension_funcs();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
