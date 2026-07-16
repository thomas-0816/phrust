<?php
// oracle-probe: id=oracle-builtin-contract-function-is-integer-1a4ffa41b8 area=builtin_contract kind=function symbol=is_integer source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-integer-1a4ffa41b8 failure_category=builtin_contract
$name = "is_integer";
echo function_exists($name) ? "available\n" : "missing\n";
