<?php
// oracle-probe: id=oracle-builtin-contract-function-str-ends-with-045352549c area=builtin_contract kind=function symbol=str_ends_with source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-str-ends-with-045352549c failure_category=builtin_contract
$name = "str_ends_with";
echo function_exists($name) ? "available\n" : "missing\n";
