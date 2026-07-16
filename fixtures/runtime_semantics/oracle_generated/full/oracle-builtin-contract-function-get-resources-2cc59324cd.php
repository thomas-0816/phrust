<?php
// oracle-probe: id=oracle-builtin-contract-function-get-resources-2cc59324cd area=builtin_contract kind=function symbol=get_resources source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-resources-2cc59324cd failure_category=builtin_contract
$name = "get_resources";
echo function_exists($name) ? "available\n" : "missing\n";
