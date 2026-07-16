<?php
// oracle-probe: id=oracle-builtin-contract-function-glob-0492ce2245 area=builtin_contract kind=function symbol=glob source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-glob-0492ce2245 failure_category=builtin_contract
$name = "glob";
echo function_exists($name) ? "available\n" : "missing\n";
