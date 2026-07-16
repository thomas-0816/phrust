<?php
// oracle-probe: id=oracle-builtin-contract-function-sem-get-075f26017f area=builtin_contract kind=function symbol=sem_get source=ext/sysvsem/sysvsem.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sem-get-075f26017f failure_category=builtin_contract requires_ref_extension=sysvsem
try {
    $result = \sem_get();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
