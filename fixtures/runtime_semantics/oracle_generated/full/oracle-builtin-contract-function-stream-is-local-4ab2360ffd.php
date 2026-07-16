<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-is-local-4ab2360ffd area=builtin_contract kind=function symbol=stream_is_local source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-is-local-4ab2360ffd failure_category=builtin_contract
$name = "stream_is_local";
echo function_exists($name) ? "available\n" : "missing\n";
