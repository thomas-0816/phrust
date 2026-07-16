<?php
// oracle-probe: id=oracle-builtin-contract-function-curl-share-init-8df9e0569b area=builtin_contract kind=function symbol=curl_share_init source=ext/curl/curl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-curl-share-init-8df9e0569b failure_category=builtin_contract requires_ref_extension=curl
try {
    $result = \curl_share_init(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
