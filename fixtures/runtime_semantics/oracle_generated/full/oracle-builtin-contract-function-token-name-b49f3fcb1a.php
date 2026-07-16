<?php
// oracle-probe: id=oracle-builtin-contract-function-token-name-b49f3fcb1a area=builtin_contract kind=function symbol=token_name source=ext/tokenizer/tokenizer.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-token-name-b49f3fcb1a failure_category=builtin_contract requires_ref_extension=tokenizer
$name = "token_name";
echo function_exists($name) ? "available\n" : "missing\n";
