<?php
// oracle-probe: id=oracle-builtin-contract-function-octdec-f9ab1c516c area=builtin_contract kind=function symbol=octdec source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-octdec-f9ab1c516c failure_category=builtin_contract
$name = "octdec";
echo function_exists($name) ? "available\n" : "missing\n";
