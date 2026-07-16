<?php
// oracle-probe: id=oracle-builtin-contract-function-fileperms-f00f00a65f area=builtin_contract kind=function symbol=fileperms source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-fileperms-f00f00a65f failure_category=builtin_contract
$name = "fileperms";
echo function_exists($name) ? "available\n" : "missing\n";
