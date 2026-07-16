<?php
// oracle-probe: id=oracle-builtin-contract-function-is-writable-7a02cc484d area=builtin_contract kind=function symbol=is_writable source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-writable-7a02cc484d failure_category=builtin_contract
$name = "is_writable";
echo function_exists($name) ? "available\n" : "missing\n";
