<?php
// oracle-probe: id=oracle-builtin-behavior-function-hash-d54d575b8b area=builtin_behavior kind=function symbol=hash source=ext/hash/hash.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-hash-d54d575b8b failure_category=builtin_behavior requires_ref_extension=hash
try {
    $result = \hash([], "");
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
