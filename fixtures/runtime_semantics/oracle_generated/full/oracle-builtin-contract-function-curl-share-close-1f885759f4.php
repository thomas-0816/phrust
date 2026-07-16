<?php
// oracle-probe: id=oracle-builtin-contract-function-curl-share-close-1f885759f4 area=builtin_contract kind=function symbol=curl_share_close source=ext/curl/curl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-curl-share-close-1f885759f4 failure_category=builtin_contract requires_ref_extension=curl
try {
    $result = \curl_share_close();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
