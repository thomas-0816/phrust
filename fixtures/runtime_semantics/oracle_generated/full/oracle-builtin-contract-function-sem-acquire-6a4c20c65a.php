<?php
// oracle-probe: id=oracle-builtin-contract-function-sem-acquire-6a4c20c65a area=builtin_contract kind=function symbol=sem_acquire source=ext/sysvsem/sysvsem.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-sem-acquire-6a4c20c65a failure_category=builtin_contract requires_ref_extension=sysvsem
try {
    $result = \sem_acquire();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
