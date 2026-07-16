<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-context-create-3319753c50 area=builtin_contract kind=function symbol=stream_context_create source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-context-create-3319753c50 failure_category=builtin_contract
$name = "stream_context_create";
echo function_exists($name) ? "available\n" : "missing\n";
