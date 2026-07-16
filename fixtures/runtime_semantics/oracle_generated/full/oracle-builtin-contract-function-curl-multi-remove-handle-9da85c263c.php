<?php
// oracle-probe: id=oracle-builtin-contract-function-curl-multi-remove-handle-9da85c263c area=builtin_contract kind=function symbol=curl_multi_remove_handle source=ext/curl/curl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-curl-multi-remove-handle-9da85c263c failure_category=builtin_contract requires_ref_extension=curl
try {
    $result = \curl_multi_remove_handle();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
