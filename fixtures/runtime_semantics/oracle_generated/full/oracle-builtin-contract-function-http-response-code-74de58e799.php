<?php
// oracle-probe: id=oracle-builtin-contract-function-http-response-code-74de58e799 area=builtin_contract kind=function symbol=http_response_code source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-http-response-code-74de58e799 failure_category=builtin_contract
$name = "http_response_code";
echo function_exists($name) ? "available\n" : "missing\n";
