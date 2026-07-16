<?php
// oracle-probe: id=oracle-builtin-contract-function-scandir-43d4cc4fee area=builtin_contract kind=function symbol=scandir source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-scandir-43d4cc4fee failure_category=builtin_contract
$name = "scandir";
echo function_exists($name) ? "available\n" : "missing\n";
