<?php
// oracle-probe: id=oracle-builtin-contract-function-curl-share-init-125f5758fc area=builtin_contract kind=function symbol=curl_share_init source=ext/curl/curl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-curl-share-init-125f5758fc failure_category=builtin_contract requires_ref_extension=curl
$name = "curl_share_init";
echo function_exists($name) ? "available\n" : "missing\n";
