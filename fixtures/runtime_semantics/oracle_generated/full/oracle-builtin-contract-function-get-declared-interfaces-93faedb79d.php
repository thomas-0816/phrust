<?php
// oracle-probe: id=oracle-builtin-contract-function-get-declared-interfaces-93faedb79d area=builtin_contract kind=function symbol=get_declared_interfaces source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-declared-interfaces-93faedb79d failure_category=builtin_contract
$name = "get_declared_interfaces";
echo function_exists($name) ? "available\n" : "missing\n";
