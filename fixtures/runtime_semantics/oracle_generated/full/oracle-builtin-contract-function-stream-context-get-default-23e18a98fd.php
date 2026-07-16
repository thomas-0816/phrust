<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-context-get-default-23e18a98fd area=builtin_contract kind=function symbol=stream_context_get_default source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-context-get-default-23e18a98fd failure_category=builtin_contract
$name = "stream_context_get_default";
echo function_exists($name) ? "available\n" : "missing\n";
