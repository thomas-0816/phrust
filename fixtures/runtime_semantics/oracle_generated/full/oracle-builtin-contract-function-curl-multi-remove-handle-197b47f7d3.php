<?php
// oracle-probe: id=oracle-builtin-contract-function-curl-multi-remove-handle-197b47f7d3 area=builtin_contract kind=function symbol=curl_multi_remove_handle source=ext/curl/curl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-curl-multi-remove-handle-197b47f7d3 failure_category=builtin_contract requires_ref_extension=curl
$name = "curl_multi_remove_handle";
echo function_exists($name) ? "available\n" : "missing\n";
