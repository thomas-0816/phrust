<?php
// oracle-probe: id=oracle-builtin-contract-function-gmp-rootrem-1ffe2f1a7a area=builtin_contract kind=function symbol=gmp_rootrem source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gmp-rootrem-1ffe2f1a7a failure_category=builtin_contract requires_ref_extension=gmp
try {
    $result = \gmp_rootrem();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
