<?php
// oracle-probe: id=oracle-builtin-contract-function-readline-completion-function-4b31f7bf4d area=builtin_contract kind=function symbol=readline_completion_function source=ext/readline/readline.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-readline-completion-function-4b31f7bf4d failure_category=builtin_contract requires_ref_extension=readline
$name = "readline_completion_function";
echo function_exists($name) ? "available\n" : "missing\n";
