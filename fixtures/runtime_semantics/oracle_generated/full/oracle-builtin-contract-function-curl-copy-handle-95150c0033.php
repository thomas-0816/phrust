<?php
// oracle-probe: id=oracle-builtin-contract-function-curl-copy-handle-95150c0033 area=builtin_contract kind=function symbol=curl_copy_handle source=ext/curl/curl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-curl-copy-handle-95150c0033 failure_category=builtin_contract requires_ref_extension=curl
try {
    $result = \curl_copy_handle();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
