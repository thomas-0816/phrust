<?php
// oracle-probe: id=oracle-builtin-contract-function-getimagesize-dd34c3e0a7 area=builtin_contract kind=function symbol=getimagesize source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-getimagesize-dd34c3e0a7 failure_category=builtin_contract
$name = "getimagesize";
echo function_exists($name) ? "available\n" : "missing\n";
