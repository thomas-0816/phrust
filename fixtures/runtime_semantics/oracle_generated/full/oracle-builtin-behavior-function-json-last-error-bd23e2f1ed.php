<?php
// oracle-probe: id=oracle-builtin-behavior-function-json-last-error-bd23e2f1ed area=builtin_behavior kind=function symbol=json_last_error source=ext/json/json.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-json-last-error-bd23e2f1ed failure_category=builtin_behavior requires_ref_extension=json
try {
    $result = \json_last_error();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
