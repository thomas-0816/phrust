<?php
// oracle-probe: id=oracle-builtin-behavior-function-sodium-increment-9fc00125ba area=builtin_behavior kind=function symbol=sodium_increment source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-sodium-increment-9fc00125ba failure_category=builtin_behavior requires_ref_extension=sodium
$arg0 = "";
try {
    $result = \sodium_increment(string: $arg0);
    echo "return:\n";
    var_dump($result);
    echo "writeback:\n";
    var_dump($arg0);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
