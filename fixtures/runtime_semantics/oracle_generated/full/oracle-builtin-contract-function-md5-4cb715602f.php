<?php
// oracle-probe: id=oracle-builtin-contract-function-md5-4cb715602f area=builtin_contract kind=function symbol=md5 source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-md5-4cb715602f failure_category=builtin_contract
$name = "md5";
echo function_exists($name) ? "available\n" : "missing\n";
