<?php
// oracle-probe: id=oracle-builtin-contract-function-shmop-size-7d58db6331 area=builtin_contract kind=function symbol=shmop_size source=ext/shmop/shmop.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-shmop-size-7d58db6331 failure_category=builtin_contract requires_ref_extension=shmop
try {
    $result = \shmop_size();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
