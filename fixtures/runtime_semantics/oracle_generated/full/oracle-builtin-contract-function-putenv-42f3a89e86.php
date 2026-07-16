<?php
// oracle-probe: id=oracle-builtin-contract-function-putenv-42f3a89e86 area=builtin_contract kind=function symbol=putenv source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-putenv-42f3a89e86 failure_category=builtin_contract
$name = "putenv";
echo function_exists($name) ? "available\n" : "missing\n";
