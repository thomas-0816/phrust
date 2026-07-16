<?php
// oracle-probe: id=oracle-builtin-contract-function-str-contains-e14dc9cd2e area=builtin_contract kind=function symbol=str_contains source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-str-contains-e14dc9cd2e failure_category=builtin_contract
$name = "str_contains";
echo function_exists($name) ? "available\n" : "missing\n";
