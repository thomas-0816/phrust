<?php
// oracle-probe: id=oracle-builtin-contract-function-intval-320cb26a3d area=builtin_contract kind=function symbol=intval source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-intval-320cb26a3d failure_category=builtin_contract
$name = "intval";
echo function_exists($name) ? "available\n" : "missing\n";
