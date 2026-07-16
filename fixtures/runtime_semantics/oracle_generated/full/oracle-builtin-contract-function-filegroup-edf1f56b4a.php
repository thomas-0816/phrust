<?php
// oracle-probe: id=oracle-builtin-contract-function-filegroup-edf1f56b4a area=builtin_contract kind=function symbol=filegroup source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-filegroup-edf1f56b4a failure_category=builtin_contract
$name = "filegroup";
echo function_exists($name) ? "available\n" : "missing\n";
