<?php
// oracle-probe: id=oracle-builtin-behavior-function-hash-init-744389e782 area=builtin_behavior kind=function symbol=hash_init source=ext/hash/hash.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-hash-init-744389e782 failure_category=builtin_behavior requires_ref_extension=hash
try {
    $result = \hash_init("", 0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
