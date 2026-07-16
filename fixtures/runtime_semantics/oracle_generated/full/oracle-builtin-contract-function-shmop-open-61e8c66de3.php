<?php
// oracle-probe: id=oracle-builtin-contract-function-shmop-open-61e8c66de3 area=builtin_contract kind=function symbol=shmop_open source=ext/shmop/shmop.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-shmop-open-61e8c66de3 failure_category=builtin_contract requires_ref_extension=shmop
try {
    $result = \shmop_open();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
