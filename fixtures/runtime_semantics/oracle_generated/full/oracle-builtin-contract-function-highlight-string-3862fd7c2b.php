<?php
// oracle-probe: id=oracle-builtin-contract-function-highlight-string-3862fd7c2b area=builtin_contract kind=function symbol=highlight_string source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-highlight-string-3862fd7c2b failure_category=builtin_contract
$name = "highlight_string";
echo function_exists($name) ? "available\n" : "missing\n";
