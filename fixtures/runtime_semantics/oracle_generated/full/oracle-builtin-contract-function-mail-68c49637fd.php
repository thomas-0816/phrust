<?php
// oracle-probe: id=oracle-builtin-contract-function-mail-68c49637fd area=builtin_contract kind=function symbol=mail source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-mail-68c49637fd failure_category=builtin_contract
$name = "mail";
echo function_exists($name) ? "available\n" : "missing\n";
