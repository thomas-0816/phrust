<?php
// oracle-probe: id=oracle-builtin-contract-function-stream-get-meta-data-63b05f1af3 area=builtin_contract kind=function symbol=stream_get_meta_data source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stream-get-meta-data-63b05f1af3 failure_category=builtin_contract
$name = "stream_get_meta_data";
echo function_exists($name) ? "available\n" : "missing\n";
