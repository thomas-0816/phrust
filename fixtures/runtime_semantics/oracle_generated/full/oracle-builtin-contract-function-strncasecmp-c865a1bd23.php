<?php
// oracle-probe: id=oracle-builtin-contract-function-strncasecmp-c865a1bd23 area=builtin_contract kind=function symbol=strncasecmp source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-strncasecmp-c865a1bd23 failure_category=builtin_contract
$name = "strncasecmp";
echo function_exists($name) ? "available\n" : "missing\n";
