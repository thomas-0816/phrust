<?php
// oracle-probe: id=oracle-builtin-contract-function-is-infinite-c1402575ea area=builtin_contract kind=function symbol=is_infinite source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-infinite-c1402575ea failure_category=builtin_contract
$name = "is_infinite";
echo function_exists($name) ? "available\n" : "missing\n";
