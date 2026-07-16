<?php
// oracle-probe: id=oracle-builtin-contract-function-is-null-b6455c74dd area=builtin_contract kind=function symbol=is_null source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-null-b6455c74dd failure_category=builtin_contract
$name = "is_null";
echo function_exists($name) ? "available\n" : "missing\n";
