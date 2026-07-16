<?php
// oracle-probe: id=oracle-builtin-contract-function-boolval-3d6a9992ed area=builtin_contract kind=function symbol=boolval source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-boolval-3d6a9992ed failure_category=builtin_contract
$name = "boolval";
echo function_exists($name) ? "available\n" : "missing\n";
