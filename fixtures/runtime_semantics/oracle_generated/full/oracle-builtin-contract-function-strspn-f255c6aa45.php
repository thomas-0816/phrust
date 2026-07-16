<?php
// oracle-probe: id=oracle-builtin-contract-function-strspn-f255c6aa45 area=builtin_contract kind=function symbol=strspn source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-strspn-f255c6aa45 failure_category=builtin_contract
$name = "strspn";
echo function_exists($name) ? "available\n" : "missing\n";
