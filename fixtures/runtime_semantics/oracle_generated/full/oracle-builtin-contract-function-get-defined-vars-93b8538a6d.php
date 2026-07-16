<?php
// oracle-probe: id=oracle-builtin-contract-function-get-defined-vars-93b8538a6d area=builtin_contract kind=function symbol=get_defined_vars source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-defined-vars-93b8538a6d failure_category=builtin_contract
$name = "get_defined_vars";
echo function_exists($name) ? "available\n" : "missing\n";
