<?php
// oracle-probe: id=oracle-builtin-contract-function-key-exists-5f47fccfca area=builtin_contract kind=function symbol=key_exists source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-key-exists-5f47fccfca failure_category=builtin_contract
$name = "key_exists";
echo function_exists($name) ? "available\n" : "missing\n";
