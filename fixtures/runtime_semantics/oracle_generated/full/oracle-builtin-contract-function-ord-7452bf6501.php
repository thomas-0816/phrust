<?php
// oracle-probe: id=oracle-builtin-contract-function-ord-7452bf6501 area=builtin_contract kind=function symbol=ord source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-ord-7452bf6501 failure_category=builtin_contract
$name = "ord";
echo function_exists($name) ? "available\n" : "missing\n";
