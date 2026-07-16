<?php
// oracle-probe: id=oracle-builtin-contract-function-dirname-9ba503d293 area=builtin_contract kind=function symbol=dirname source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-dirname-9ba503d293 failure_category=builtin_contract
$name = "dirname";
echo function_exists($name) ? "available\n" : "missing\n";
