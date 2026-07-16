<?php
// oracle-probe: id=oracle-builtin-contract-function-wordwrap-a0efdad521 area=builtin_contract kind=function symbol=wordwrap source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-wordwrap-a0efdad521 failure_category=builtin_contract
$name = "wordwrap";
echo function_exists($name) ? "available\n" : "missing\n";
