<?php
// oracle-probe: id=oracle-builtin-contract-function-end-5762e76771 area=builtin_contract kind=function symbol=end source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-end-5762e76771 failure_category=builtin_contract
$name = "end";
echo function_exists($name) ? "available\n" : "missing\n";
