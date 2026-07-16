<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-context-get-options-305b4c0e32 area=builtin_contract kind=function symbol=stream_context_get_options source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-context-get-options-305b4c0e32 failure_category=builtin_contract
$name = "stream_context_get_options";
echo function_exists($name) ? "available\n" : "missing\n";
