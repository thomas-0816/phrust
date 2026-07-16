<?php
// oracle-probe: id=oracle-builtin-contract-function-gettype-b11562f4fc area=builtin_contract kind=function symbol=gettype source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-gettype-b11562f4fc failure_category=builtin_contract
$name = "gettype";
echo function_exists($name) ? "available\n" : "missing\n";
