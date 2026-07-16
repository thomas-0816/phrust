<?php
// oracle-probe: id=oracle-builtin-contract-function-substr-replace-8f2b8484ed area=builtin_contract kind=function symbol=substr_replace source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-substr-replace-8f2b8484ed failure_category=builtin_contract
$name = "substr_replace";
echo function_exists($name) ? "available\n" : "missing\n";
