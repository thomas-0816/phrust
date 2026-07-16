<?php
// oracle-probe: id=oracle-builtin-contract-function-filemtime-d7cf1c0a5f area=builtin_contract kind=function symbol=filemtime source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-filemtime-d7cf1c0a5f failure_category=builtin_contract
$name = "filemtime";
echo function_exists($name) ? "available\n" : "missing\n";
