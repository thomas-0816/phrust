<?php
// oracle-probe: id=oracle-builtin-contract-function-var-export-cfc0bebdbf area=builtin_contract kind=function symbol=var_export source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-var-export-cfc0bebdbf failure_category=builtin_contract
$name = "var_export";
echo function_exists($name) ? "available\n" : "missing\n";
