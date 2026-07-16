<?php
// oracle-probe: id=oracle-builtin-contract-function-gmp-div-qr-332a0bb5a9 area=builtin_contract kind=function symbol=gmp_div_qr source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gmp-div-qr-332a0bb5a9 failure_category=builtin_contract requires_ref_extension=gmp
try {
    $result = \gmp_div_qr();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
