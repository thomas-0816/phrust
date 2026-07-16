<?php
// oracle-probe: id=oracle-builtin-contract-function-getmyuid-b9091bfbcc area=builtin_contract kind=function symbol=getmyuid source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-getmyuid-b9091bfbcc failure_category=builtin_contract
$name = "getmyuid";
echo function_exists($name) ? "available\n" : "missing\n";
