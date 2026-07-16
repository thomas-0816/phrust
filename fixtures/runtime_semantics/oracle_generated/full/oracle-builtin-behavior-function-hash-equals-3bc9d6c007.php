<?php
// oracle-probe: id=oracle-builtin-behavior-function-hash-equals-3bc9d6c007 area=builtin_behavior kind=function symbol=hash_equals source=ext/hash/hash.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-hash-equals-3bc9d6c007 failure_category=builtin_behavior requires_ref_extension=hash
try {
    $result = \hash_equals(known_string: "", user_string: "");
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
