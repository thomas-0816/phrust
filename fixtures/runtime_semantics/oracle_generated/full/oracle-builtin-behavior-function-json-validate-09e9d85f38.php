<?php
// oracle-probe: id=oracle-builtin-behavior-function-json-validate-09e9d85f38 area=builtin_behavior kind=function symbol=json_validate source=ext/json/json.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-json-validate-09e9d85f38 failure_category=builtin_behavior requires_ref_extension=json
try {
    $result = \json_validate([]);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
