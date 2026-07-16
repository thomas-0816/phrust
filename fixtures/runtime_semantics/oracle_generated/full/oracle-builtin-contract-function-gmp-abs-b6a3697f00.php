<?php
// oracle-probe: id=oracle-builtin-contract-function-gmp-abs-b6a3697f00 area=builtin_contract kind=function symbol=gmp_abs source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gmp-abs-b6a3697f00 failure_category=builtin_contract requires_ref_extension=gmp
try {
    $result = \gmp_abs();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
