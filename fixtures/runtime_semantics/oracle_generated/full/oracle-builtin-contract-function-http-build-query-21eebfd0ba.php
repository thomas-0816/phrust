<?php
// oracle-probe: id=oracle-builtin-contract-function-http-build-query-21eebfd0ba area=builtin_contract kind=function symbol=http_build_query source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-http-build-query-21eebfd0ba failure_category=builtin_contract
$name = "http_build_query";
echo function_exists($name) ? "available\n" : "missing\n";
