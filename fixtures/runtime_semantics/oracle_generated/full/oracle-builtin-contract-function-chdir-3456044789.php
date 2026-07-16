<?php
// oracle-probe: id=oracle-builtin-contract-function-chdir-3456044789 area=builtin_contract kind=function symbol=chdir source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-chdir-3456044789 failure_category=builtin_contract
$name = "chdir";
echo function_exists($name) ? "available\n" : "missing\n";
