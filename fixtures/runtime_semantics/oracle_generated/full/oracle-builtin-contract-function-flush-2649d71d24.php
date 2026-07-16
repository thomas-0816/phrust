<?php
// oracle-probe: id=oracle-builtin-contract-function-flush-2649d71d24 area=builtin_contract kind=function symbol=flush source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-flush-2649d71d24 failure_category=builtin_contract
$name = "flush";
echo function_exists($name) ? "available\n" : "missing\n";
