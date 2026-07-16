<?php
// oracle-probe: id=oracle-builtin-contract-function-sizeof-d3fb30b76f area=builtin_contract kind=function symbol=sizeof source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-sizeof-d3fb30b76f failure_category=builtin_contract
$name = "sizeof";
echo function_exists($name) ? "available\n" : "missing\n";
