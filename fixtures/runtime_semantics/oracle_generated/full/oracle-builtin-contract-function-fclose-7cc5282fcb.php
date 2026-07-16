<?php
// oracle-probe: id=oracle-builtin-contract-function-fclose-7cc5282fcb area=builtin_contract kind=function symbol=fclose source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-fclose-7cc5282fcb failure_category=builtin_contract
$name = "fclose";
echo function_exists($name) ? "available\n" : "missing\n";
