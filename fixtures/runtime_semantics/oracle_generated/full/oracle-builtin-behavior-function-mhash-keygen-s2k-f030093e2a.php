<?php
// oracle-probe: id=oracle-builtin-behavior-function-mhash-keygen-s2k-f030093e2a area=builtin_behavior kind=function symbol=mhash_keygen_s2k source=ext/hash/hash.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-mhash-keygen-s2k-f030093e2a failure_category=builtin_behavior requires_ref_extension=hash
try {
    $result = \mhash_keygen_s2k([], "", "", 0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
