<?php
// oracle-probe: id=oracle-builtin-contract-function-getcwd-c04d418697 area=builtin_contract kind=function symbol=getcwd source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-getcwd-c04d418697 failure_category=builtin_contract
$name = "getcwd";
echo function_exists($name) ? "available\n" : "missing\n";
