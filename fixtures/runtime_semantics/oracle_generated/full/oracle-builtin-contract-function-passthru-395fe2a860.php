<?php
// oracle-probe: id=oracle-builtin-contract-function-passthru-395fe2a860 area=builtin_contract kind=function symbol=passthru source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-passthru-395fe2a860 failure_category=builtin_contract
$name = "passthru";
echo function_exists($name) ? "available\n" : "missing\n";
