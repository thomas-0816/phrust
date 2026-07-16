<?php
// oracle-probe: id=oracle-builtin-contract-function-shmop-delete-2c9e4a2876 area=builtin_contract kind=function symbol=shmop_delete source=ext/shmop/shmop.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-shmop-delete-2c9e4a2876 failure_category=builtin_contract requires_ref_extension=shmop
try {
    $result = \shmop_delete();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
