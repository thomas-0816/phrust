<?php
// oracle-probe: id=oracle-builtin-contract-function-array-search-370f280b7f area=builtin_contract kind=function symbol=array_search source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-search-370f280b7f failure_category=builtin_contract
$name = "array_search";
echo function_exists($name) ? "available\n" : "missing\n";
