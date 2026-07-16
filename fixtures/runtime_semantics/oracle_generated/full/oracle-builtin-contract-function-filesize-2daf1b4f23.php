<?php
// oracle-probe: id=oracle-builtin-contract-function-filesize-2daf1b4f23 area=builtin_contract kind=function symbol=filesize source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-filesize-2daf1b4f23 failure_category=builtin_contract
$name = "filesize";
echo function_exists($name) ? "available\n" : "missing\n";
