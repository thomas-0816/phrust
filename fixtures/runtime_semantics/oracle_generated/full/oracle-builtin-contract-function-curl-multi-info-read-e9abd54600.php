<?php
// oracle-probe: id=oracle-builtin-contract-function-curl-multi-info-read-e9abd54600 area=builtin_contract kind=function symbol=curl_multi_info_read source=ext/curl/curl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-curl-multi-info-read-e9abd54600 failure_category=builtin_contract requires_ref_extension=curl
$name = "curl_multi_info_read";
echo function_exists($name) ? "available\n" : "missing\n";
