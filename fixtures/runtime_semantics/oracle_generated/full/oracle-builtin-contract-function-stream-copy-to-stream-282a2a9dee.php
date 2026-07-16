<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-copy-to-stream-282a2a9dee area=builtin_contract kind=function symbol=stream_copy_to_stream source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-copy-to-stream-282a2a9dee failure_category=builtin_contract
$name = "stream_copy_to_stream";
echo function_exists($name) ? "available\n" : "missing\n";
