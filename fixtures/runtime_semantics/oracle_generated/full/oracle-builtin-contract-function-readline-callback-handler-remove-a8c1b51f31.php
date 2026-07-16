<?php
// oracle-probe: id=oracle-builtin-contract-function-readline-callback-handler-remove-a8c1b51f31 area=builtin_contract kind=function symbol=readline_callback_handler_remove source=ext/readline/readline.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-readline-callback-handler-remove-a8c1b51f31 failure_category=builtin_contract requires_ref_extension=readline
$name = "readline_callback_handler_remove";
echo function_exists($name) ? "available\n" : "missing\n";
