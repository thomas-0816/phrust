<?php
// oracle-probe: id=oracle-builtin-behavior-function-gmp-invert-4e8c4af0e8 area=builtin_behavior kind=function symbol=gmp_invert source=ext/gmp/gmp.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-gmp-invert-4e8c4af0e8 failure_category=builtin_behavior requires_ref_extension=gmp
try {
    $result = \gmp_invert(0, 0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
