<?php
// oracle-probe: id=oracle-builtin-contract-function-str-split-a672f55b02 area=builtin_contract kind=function symbol=str_split source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-str-split-a672f55b02 failure_category=builtin_contract
$name = "str_split";
echo function_exists($name) ? "available\n" : "missing\n";
