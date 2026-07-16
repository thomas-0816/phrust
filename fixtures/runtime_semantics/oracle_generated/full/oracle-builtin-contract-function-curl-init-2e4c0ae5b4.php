<?php
// oracle-probe: id=oracle-builtin-contract-function-curl-init-2e4c0ae5b4 area=builtin_contract kind=function symbol=curl_init source=ext/curl/curl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-curl-init-2e4c0ae5b4 failure_category=builtin_contract requires_ref_extension=curl
$name = "curl_init";
echo function_exists($name) ? "available\n" : "missing\n";
