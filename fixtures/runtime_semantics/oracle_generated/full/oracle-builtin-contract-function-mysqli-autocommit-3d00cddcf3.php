<?php
// oracle-probe: id=oracle-builtin-contract-function-mysqli-autocommit-3d00cddcf3 area=builtin_contract kind=function symbol=mysqli_autocommit source=ext/mysqli/mysqli.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mysqli-autocommit-3d00cddcf3 failure_category=builtin_contract requires_ref_extension=mysqli
try {
    $result = \mysqli_autocommit();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
