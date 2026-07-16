<?php
// oracle-probe: id=oracle-builtin-contract-function-readline-callback-read-char-714014ccd3 area=builtin_contract kind=function symbol=readline_callback_read_char source=ext/readline/readline.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-readline-callback-read-char-714014ccd3 failure_category=builtin_contract requires_ref_extension=readline
$name = "readline_callback_read_char";
echo function_exists($name) ? "available\n" : "missing\n";
