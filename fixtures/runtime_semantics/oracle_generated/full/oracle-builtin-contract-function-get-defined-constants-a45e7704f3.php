<?php
// oracle-probe: id=oracle-builtin-contract-function-get-defined-constants-a45e7704f3 area=builtin_contract kind=function symbol=get_defined_constants source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-defined-constants-a45e7704f3 failure_category=builtin_contract
$name = "get_defined_constants";
echo function_exists($name) ? "available\n" : "missing\n";
