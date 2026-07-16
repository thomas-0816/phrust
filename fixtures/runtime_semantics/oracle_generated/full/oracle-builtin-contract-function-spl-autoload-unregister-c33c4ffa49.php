<?php
// oracle-probe: id=oracle-builtin-contract-function-spl-autoload-unregister-c33c4ffa49 area=builtin_contract kind=function symbol=spl_autoload_unregister source=ext/spl/php_spl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-spl-autoload-unregister-c33c4ffa49 failure_category=builtin_contract requires_ref_extension=spl
try {
    $result = \spl_autoload_unregister();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
