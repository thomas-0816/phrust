<?php
// oracle-probe: id=oracle-builtin-contract-function-basename-2b7839cc0d area=builtin_contract kind=function symbol=basename source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-basename-2b7839cc0d failure_category=builtin_contract
$name = "basename";
echo function_exists($name) ? "available\n" : "missing\n";
