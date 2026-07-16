<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-get-contents-ae1f956694 area=builtin_contract kind=function symbol=stream_get_contents source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-get-contents-ae1f956694 failure_category=builtin_contract
$name = "stream_get_contents";
echo function_exists($name) ? "available\n" : "missing\n";
