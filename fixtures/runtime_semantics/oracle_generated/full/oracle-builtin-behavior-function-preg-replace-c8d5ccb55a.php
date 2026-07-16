<?php
// oracle-probe: id=oracle-builtin-behavior-function-preg-replace-c8d5ccb55a area=builtin_behavior kind=function symbol=preg_replace source=ext/pcre/php_pcre.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-preg-replace-c8d5ccb55a failure_category=builtin_behavior requires_ref_extension=pcre
try {
    $result = \preg_replace("", "", "", 0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
