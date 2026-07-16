<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-set-timeout-460161eb9a area=builtin_contract kind=function symbol=stream_set_timeout source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-set-timeout-460161eb9a failure_category=builtin_contract
$name = "stream_set_timeout";
echo function_exists($name) ? "available\n" : "missing\n";
