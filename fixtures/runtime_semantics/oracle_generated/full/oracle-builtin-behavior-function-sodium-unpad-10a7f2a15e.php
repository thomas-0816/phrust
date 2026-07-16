<?php
// oracle-probe: id=oracle-builtin-behavior-function-sodium-unpad-10a7f2a15e area=builtin_behavior kind=function symbol=sodium_unpad source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-sodium-unpad-10a7f2a15e failure_category=builtin_behavior requires_ref_extension=sodium
try {
    $result = \sodium_unpad(string: "", block_size: 0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
