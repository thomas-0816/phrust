<?php
// oracle-probe: id=oracle-builtin-behavior-function-sodium-pad-1f29d0040a area=builtin_behavior kind=function symbol=sodium_pad source=ext/sodium/libsodium.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-sodium-pad-1f29d0040a failure_category=builtin_behavior requires_ref_extension=sodium
try {
    $result = \sodium_pad(string: "", block_size: 0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
