<?php
// oracle-probe: id=oracle-builtin-contract-function-shmop-read-2d1494016c area=builtin_contract kind=function symbol=shmop_read source=ext/shmop/shmop.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-shmop-read-2d1494016c failure_category=builtin_contract requires_ref_extension=shmop
try {
    $result = \shmop_read();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
