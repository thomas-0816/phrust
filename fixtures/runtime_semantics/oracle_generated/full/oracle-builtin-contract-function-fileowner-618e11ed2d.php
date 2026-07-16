<?php
// oracle-probe: id=oracle-builtin-contract-function-fileowner-618e11ed2d area=builtin_contract kind=function symbol=fileowner source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-fileowner-618e11ed2d failure_category=builtin_contract
$name = "fileowner";
echo function_exists($name) ? "available\n" : "missing\n";
