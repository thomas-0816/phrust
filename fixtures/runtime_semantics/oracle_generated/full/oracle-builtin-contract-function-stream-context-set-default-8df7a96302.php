<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-context-set-default-8df7a96302 area=builtin_contract kind=function symbol=stream_context_set_default source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-context-set-default-8df7a96302 failure_category=builtin_contract
$name = "stream_context_set_default";
echo function_exists($name) ? "available\n" : "missing\n";
