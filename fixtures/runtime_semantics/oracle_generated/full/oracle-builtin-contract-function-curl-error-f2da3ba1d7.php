<?php
// oracle-probe: id=oracle-builtin-contract-function-curl-error-f2da3ba1d7 area=builtin_contract kind=function symbol=curl_error source=ext/curl/curl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-curl-error-f2da3ba1d7 failure_category=builtin_contract requires_ref_extension=curl
try {
    $result = \curl_error();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
