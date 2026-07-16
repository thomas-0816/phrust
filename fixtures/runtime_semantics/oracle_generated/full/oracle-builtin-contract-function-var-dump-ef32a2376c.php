<?php
// oracle-probe: id=oracle-builtin-contract-function-var-dump-ef32a2376c area=builtin_contract kind=function symbol=var_dump source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-var-dump-ef32a2376c failure_category=builtin_contract
$name = "var_dump";
echo function_exists($name) ? "available\n" : "missing\n";
