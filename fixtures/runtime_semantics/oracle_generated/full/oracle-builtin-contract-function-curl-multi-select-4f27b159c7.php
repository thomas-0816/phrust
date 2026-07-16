<?php
// oracle-probe: id=oracle-builtin-contract-function-curl-multi-select-4f27b159c7 area=builtin_contract kind=function symbol=curl_multi_select source=ext/curl/curl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-curl-multi-select-4f27b159c7 failure_category=builtin_contract requires_ref_extension=curl
$name = "curl_multi_select";
echo function_exists($name) ? "available\n" : "missing\n";
