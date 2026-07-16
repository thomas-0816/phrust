<?php
// oracle-probe: id=oracle-builtin-contract-function-explode-40dffa692d area=builtin_contract kind=function symbol=explode source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-explode-40dffa692d failure_category=builtin_contract
$name = "explode";
echo function_exists($name) ? "available\n" : "missing\n";
