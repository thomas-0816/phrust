<?php
// oracle-probe: id=oracle-builtin-contract-function-token-get-all-115ff5d2b5 area=builtin_contract kind=function symbol=token_get_all source=ext/tokenizer/tokenizer.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-token-get-all-115ff5d2b5 failure_category=builtin_contract requires_ref_extension=tokenizer
$name = "token_get_all";
echo function_exists($name) ? "available\n" : "missing\n";
