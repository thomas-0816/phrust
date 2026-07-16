<?php
// oracle-probe: id=oracle-builtin-contract-function-chown-57bd47e78b area=builtin_contract kind=function symbol=chown source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-chown-57bd47e78b failure_category=builtin_contract
$name = "chown";
echo function_exists($name) ? "available\n" : "missing\n";
