<?php
// oracle-probe: id=oracle-builtin-contract-function-tmpfile-f2e39ca0b9 area=builtin_contract kind=function symbol=tmpfile source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-tmpfile-f2e39ca0b9 failure_category=builtin_contract
$name = "tmpfile";
echo function_exists($name) ? "available\n" : "missing\n";
