<?php
// oracle-probe: id=oracle-builtin-contract-function-readline-09934c6cfa area=builtin_contract kind=function symbol=readline source=ext/readline/readline.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-readline-09934c6cfa failure_category=builtin_contract requires_ref_extension=readline
$name = "readline";
echo function_exists($name) ? "available\n" : "missing\n";
