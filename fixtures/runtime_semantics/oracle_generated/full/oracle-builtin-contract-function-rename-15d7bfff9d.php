<?php
// oracle-probe: id=oracle-builtin-contract-function-rename-15d7bfff9d area=builtin_contract kind=function symbol=rename source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-rename-15d7bfff9d failure_category=builtin_contract
$name = "rename";
echo function_exists($name) ? "available\n" : "missing\n";
