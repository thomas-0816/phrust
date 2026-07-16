<?php
// oracle-probe: id=oracle-builtin-contract-function-version-compare-0a54d86bf1 area=builtin_contract kind=function symbol=version_compare source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-version-compare-0a54d86bf1 failure_category=builtin_contract
$name = "version_compare";
echo function_exists($name) ? "available\n" : "missing\n";
