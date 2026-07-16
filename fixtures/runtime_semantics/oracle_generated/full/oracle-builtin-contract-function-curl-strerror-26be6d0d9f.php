<?php
// oracle-probe: id=oracle-builtin-contract-function-curl-strerror-26be6d0d9f area=builtin_contract kind=function symbol=curl_strerror source=ext/curl/curl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-curl-strerror-26be6d0d9f failure_category=builtin_contract requires_ref_extension=curl
try {
    $result = \curl_strerror();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
