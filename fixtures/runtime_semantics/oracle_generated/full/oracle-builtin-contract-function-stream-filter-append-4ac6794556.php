<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-filter-append-4ac6794556 area=builtin_contract kind=function symbol=stream_filter_append source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-filter-append-4ac6794556 failure_category=builtin_contract
$name = "stream_filter_append";
echo function_exists($name) ? "available\n" : "missing\n";
