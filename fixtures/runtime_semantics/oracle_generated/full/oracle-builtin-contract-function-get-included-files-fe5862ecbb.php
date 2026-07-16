<?php
// oracle-probe: id=oracle-builtin-contract-function-get-included-files-fe5862ecbb area=builtin_contract kind=function symbol=get_included_files source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-included-files-fe5862ecbb failure_category=builtin_contract
$name = "get_included_files";
echo function_exists($name) ? "available\n" : "missing\n";
