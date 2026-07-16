<?php
// oracle-probe: id=oracle-builtin-contract-function-readline-read-history-56cb6a0a1a area=builtin_contract kind=function symbol=readline_read_history source=ext/readline/readline.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-readline-read-history-56cb6a0a1a failure_category=builtin_contract requires_ref_extension=readline
$name = "readline_read_history";
echo function_exists($name) ? "available\n" : "missing\n";
