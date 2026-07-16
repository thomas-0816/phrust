<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-filter-register-24c56eaabd area=builtin_contract kind=function symbol=stream_filter_register source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-filter-register-24c56eaabd failure_category=builtin_contract
$name = "stream_filter_register";
echo function_exists($name) ? "available\n" : "missing\n";
