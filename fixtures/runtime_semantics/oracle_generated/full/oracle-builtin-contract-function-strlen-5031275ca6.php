<?php
// oracle-probe: id=oracle-builtin-contract-function-strlen-5031275ca6 area=builtin_contract kind=function symbol=strlen source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-strlen-5031275ca6 failure_category=builtin_contract
$name = "strlen";
echo function_exists($name) ? "available\n" : "missing\n";
