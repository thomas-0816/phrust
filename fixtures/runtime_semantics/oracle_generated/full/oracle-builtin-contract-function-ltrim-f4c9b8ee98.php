<?php
// oracle-probe: id=oracle-builtin-contract-function-ltrim-f4c9b8ee98 area=builtin_contract kind=function symbol=ltrim source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-ltrim-f4c9b8ee98 failure_category=builtin_contract
$name = "ltrim";
echo function_exists($name) ? "available\n" : "missing\n";
