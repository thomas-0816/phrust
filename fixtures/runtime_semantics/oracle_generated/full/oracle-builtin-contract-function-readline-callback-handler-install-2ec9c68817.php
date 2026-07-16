<?php
// oracle-probe: id=oracle-builtin-contract-function-readline-callback-handler-install-2ec9c68817 area=builtin_contract kind=function symbol=readline_callback_handler_install source=ext/readline/readline.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-readline-callback-handler-install-2ec9c68817 failure_category=builtin_contract requires_ref_extension=readline
$name = "readline_callback_handler_install";
echo function_exists($name) ? "available\n" : "missing\n";
