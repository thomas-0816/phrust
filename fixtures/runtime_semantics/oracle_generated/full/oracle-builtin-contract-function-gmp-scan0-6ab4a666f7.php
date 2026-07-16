<?php
// oracle-probe: id=oracle-builtin-contract-function-gmp-scan0-6ab4a666f7 area=builtin_contract kind=function symbol=gmp_scan0 source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gmp-scan0-6ab4a666f7 failure_category=builtin_contract requires_ref_extension=gmp
try {
    $result = \gmp_scan0();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
