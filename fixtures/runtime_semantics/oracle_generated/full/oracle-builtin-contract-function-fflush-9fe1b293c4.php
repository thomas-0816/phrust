<?php
// oracle-probe: id=oracle-builtin-contract-function-fflush-9fe1b293c4 area=builtin_contract kind=function symbol=fflush source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-fflush-9fe1b293c4 failure_category=builtin_contract
$name = "fflush";
echo function_exists($name) ? "available\n" : "missing\n";
