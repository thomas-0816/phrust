<?php
// oracle-probe: id=oracle-builtin-contract-function-readline-on-new-line-0cdf434e0a area=builtin_contract kind=function symbol=readline_on_new_line source=ext/readline/readline.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-readline-on-new-line-0cdf434e0a failure_category=builtin_contract requires_ref_extension=readline
$name = "readline_on_new_line";
echo function_exists($name) ? "available\n" : "missing\n";
