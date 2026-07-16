<?php
// oracle-probe: id=oracle-builtin-behavior-function-json-encode-329c986363 area=builtin_behavior kind=function symbol=json_encode source=ext/json/json.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-json-encode-329c986363 failure_category=builtin_behavior requires_ref_extension=json
try {
    $result = \json_encode(value: null);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
