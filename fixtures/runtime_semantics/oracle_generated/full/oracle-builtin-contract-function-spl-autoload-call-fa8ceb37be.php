<?php
// oracle-probe: id=oracle-builtin-contract-function-spl-autoload-call-fa8ceb37be area=builtin_contract kind=function symbol=spl_autoload_call source=ext/spl/php_spl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-spl-autoload-call-fa8ceb37be failure_category=builtin_contract requires_ref_extension=spl
try {
    $result = \spl_autoload_call();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
