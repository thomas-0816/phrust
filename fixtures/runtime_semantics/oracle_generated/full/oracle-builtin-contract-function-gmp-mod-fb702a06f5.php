<?php
// oracle-probe: id=oracle-builtin-contract-function-gmp-mod-fb702a06f5 area=builtin_contract kind=function symbol=gmp_mod source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gmp-mod-fb702a06f5 failure_category=builtin_contract requires_ref_extension=gmp
try {
    $result = \gmp_mod();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
