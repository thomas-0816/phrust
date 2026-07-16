<?php
// oracle-probe: id=oracle-builtin-contract-function-htmlspecialchars-12d590fff2 area=builtin_contract kind=function symbol=htmlspecialchars source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-htmlspecialchars-12d590fff2 failure_category=builtin_contract
$name = "htmlspecialchars";
echo function_exists($name) ? "available\n" : "missing\n";
