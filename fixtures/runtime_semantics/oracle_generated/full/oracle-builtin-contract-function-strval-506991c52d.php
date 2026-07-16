<?php
// oracle-probe: id=oracle-builtin-contract-function-strval-506991c52d area=builtin_contract kind=function symbol=strval source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-strval-506991c52d failure_category=builtin_contract
$name = "strval";
echo function_exists($name) ? "available\n" : "missing\n";
