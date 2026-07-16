<?php
// oracle-probe: id=oracle-builtin-contract-function-curl-errno-668a41b489 area=builtin_contract kind=function symbol=curl_errno source=ext/curl/curl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-curl-errno-668a41b489 failure_category=builtin_contract requires_ref_extension=curl
$name = "curl_errno";
echo function_exists($name) ? "available\n" : "missing\n";
