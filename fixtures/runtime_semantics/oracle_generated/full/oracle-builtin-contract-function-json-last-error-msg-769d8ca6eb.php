<?php
// oracle-probe: id=oracle-builtin-contract-function-json-last-error-msg-769d8ca6eb area=builtin_contract kind=function symbol=json_last_error_msg source=ext/json/json.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-json-last-error-msg-769d8ca6eb failure_category=builtin_contract requires_ref_extension=json
try {
    $result = \json_last_error_msg(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
