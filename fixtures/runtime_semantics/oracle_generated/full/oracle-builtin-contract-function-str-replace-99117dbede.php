<?php
// oracle-probe: id=oracle-builtin-contract-function-str-replace-99117dbede area=builtin_contract kind=function symbol=str_replace source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-str-replace-99117dbede failure_category=builtin_contract
$name = "str_replace";
echo function_exists($name) ? "available\n" : "missing\n";
