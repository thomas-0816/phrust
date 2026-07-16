<?php
// oracle-probe: id=oracle-builtin-contract-function-random-bytes-bacf888510 area=builtin_contract kind=function symbol=random_bytes source=ext/random/random.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-random-bytes-bacf888510 failure_category=builtin_contract requires_ref_extension=random
try {
    $result = \random_bytes();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
