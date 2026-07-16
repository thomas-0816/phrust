<?php
// oracle-probe: id=oracle-builtin-contract-function-getenv-eade2b9597 area=builtin_contract kind=function symbol=getenv source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-getenv-eade2b9597 failure_category=builtin_contract
$name = "getenv";
echo function_exists($name) ? "available\n" : "missing\n";
