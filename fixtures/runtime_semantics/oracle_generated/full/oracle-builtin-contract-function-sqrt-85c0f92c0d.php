<?php
// oracle-probe: id=oracle-builtin-contract-function-sqrt-85c0f92c0d area=builtin_contract kind=function symbol=sqrt source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-sqrt-85c0f92c0d failure_category=builtin_contract
$name = "sqrt";
echo function_exists($name) ? "available\n" : "missing\n";
