<?php
// oracle-probe: id=oracle-builtin-contract-function-gmp-random-seed-bad24aaaea area=builtin_contract kind=function symbol=gmp_random_seed source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gmp-random-seed-bad24aaaea failure_category=builtin_contract requires_ref_extension=gmp
try {
    $result = \gmp_random_seed();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
