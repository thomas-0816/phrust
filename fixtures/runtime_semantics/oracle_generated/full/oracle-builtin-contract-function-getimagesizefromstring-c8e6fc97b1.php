<?php
// oracle-probe: id=oracle-builtin-contract-function-getimagesizefromstring-c8e6fc97b1 area=builtin_contract kind=function symbol=getimagesizefromstring source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-getimagesizefromstring-c8e6fc97b1 failure_category=builtin_contract
$name = "getimagesizefromstring";
echo function_exists($name) ? "available\n" : "missing\n";
