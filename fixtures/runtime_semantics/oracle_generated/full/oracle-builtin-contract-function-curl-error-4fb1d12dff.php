<?php
// oracle-probe: id=oracle-builtin-contract-function-curl-error-4fb1d12dff area=builtin_contract kind=function symbol=curl_error source=ext/curl/curl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-curl-error-4fb1d12dff failure_category=builtin_contract requires_ref_extension=curl
$name = "curl_error";
echo function_exists($name) ? "available\n" : "missing\n";
