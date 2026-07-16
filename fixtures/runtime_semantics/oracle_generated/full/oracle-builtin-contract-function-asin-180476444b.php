<?php
// oracle-probe: id=oracle-builtin-contract-function-asin-180476444b area=builtin_contract kind=function symbol=asin source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-asin-180476444b failure_category=builtin_contract
$name = "asin";
echo function_exists($name) ? "available\n" : "missing\n";
