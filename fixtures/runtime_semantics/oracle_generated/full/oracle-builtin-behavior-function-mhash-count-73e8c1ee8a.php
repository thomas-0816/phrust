<?php
// oracle-probe: id=oracle-builtin-behavior-function-mhash-count-73e8c1ee8a area=builtin_behavior kind=function symbol=mhash_count source=ext/hash/hash.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-mhash-count-73e8c1ee8a failure_category=builtin_behavior requires_ref_extension=hash
try {
    $result = \mhash_count();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
