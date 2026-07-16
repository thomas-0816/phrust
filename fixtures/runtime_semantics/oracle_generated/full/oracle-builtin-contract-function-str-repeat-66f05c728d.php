<?php
// oracle-probe: id=oracle-builtin-contract-function-str-repeat-66f05c728d area=builtin_contract kind=function symbol=str_repeat source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-str-repeat-66f05c728d failure_category=builtin_contract
$name = "str_repeat";
echo function_exists($name) ? "available\n" : "missing\n";
