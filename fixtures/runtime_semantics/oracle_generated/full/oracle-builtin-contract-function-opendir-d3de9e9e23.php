<?php
// oracle-probe: id=oracle-builtin-contract-function-opendir-d3de9e9e23 area=builtin_contract kind=function symbol=opendir source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-opendir-d3de9e9e23 failure_category=builtin_contract
$name = "opendir";
echo function_exists($name) ? "available\n" : "missing\n";
