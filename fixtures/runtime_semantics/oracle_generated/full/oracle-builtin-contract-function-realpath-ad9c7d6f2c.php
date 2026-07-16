<?php
// oracle-probe: id=oracle-builtin-contract-function-realpath-ad9c7d6f2c area=builtin_contract kind=function symbol=realpath source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-realpath-ad9c7d6f2c failure_category=builtin_contract
$name = "realpath";
echo function_exists($name) ? "available\n" : "missing\n";
