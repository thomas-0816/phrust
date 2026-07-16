<?php
// oracle-probe: id=oracle-builtin-contract-function-defined-9b7693ec4b area=builtin_contract kind=function symbol=defined source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-defined-9b7693ec4b failure_category=builtin_contract
$name = "defined";
echo function_exists($name) ? "available\n" : "missing\n";
