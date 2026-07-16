<?php
// oracle-probe: id=oracle-builtin-contract-function-error-reporting-0eb648b1d5 area=builtin_contract kind=function symbol=error_reporting source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-error-reporting-0eb648b1d5 failure_category=builtin_contract
$name = "error_reporting";
echo function_exists($name) ? "available\n" : "missing\n";
