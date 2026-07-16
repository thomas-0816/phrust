<?php
// oracle-probe: id=oracle-builtin-contract-function-random-int-52f89d3714 area=builtin_contract kind=function symbol=random_int source=ext/random/random.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-random-int-52f89d3714 failure_category=builtin_contract requires_ref_extension=random
try {
    $result = \random_int();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
