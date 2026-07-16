<?php
// oracle-probe: id=oracle-builtin-contract-function-feof-340ef8dc1f area=builtin_contract kind=function symbol=feof source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-feof-340ef8dc1f failure_category=builtin_contract
$name = "feof";
echo function_exists($name) ? "available\n" : "missing\n";
