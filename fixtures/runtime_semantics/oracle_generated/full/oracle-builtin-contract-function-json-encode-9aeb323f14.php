<?php
// oracle-probe: id=oracle-builtin-contract-function-json-encode-9aeb323f14 area=builtin_contract kind=function symbol=json_encode source=ext/json/json.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-json-encode-9aeb323f14 failure_category=builtin_contract requires_ref_extension=json
try {
    $result = \json_encode();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
