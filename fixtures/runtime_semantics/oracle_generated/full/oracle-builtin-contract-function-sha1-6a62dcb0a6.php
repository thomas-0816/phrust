<?php
// oracle-probe: id=oracle-builtin-contract-function-sha1-6a62dcb0a6 area=builtin_contract kind=function symbol=sha1 source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-sha1-6a62dcb0a6 failure_category=builtin_contract
$name = "sha1";
echo function_exists($name) ? "available\n" : "missing\n";
