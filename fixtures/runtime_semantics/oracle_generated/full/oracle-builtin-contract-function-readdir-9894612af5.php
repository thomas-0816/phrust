<?php
// oracle-probe: id=oracle-builtin-contract-function-readdir-9894612af5 area=builtin_contract kind=function symbol=readdir source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-readdir-9894612af5 failure_category=builtin_contract
$name = "readdir";
echo function_exists($name) ? "available\n" : "missing\n";
