<?php
// oracle-probe: id=oracle-builtin-contract-function-is-string-86bc8ac8de area=builtin_contract kind=function symbol=is_string source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-string-86bc8ac8de failure_category=builtin_contract
$name = "is_string";
echo function_exists($name) ? "available\n" : "missing\n";
