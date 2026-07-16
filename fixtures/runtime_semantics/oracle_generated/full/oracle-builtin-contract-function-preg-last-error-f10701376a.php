<?php
// oracle-probe: id=oracle-builtin-contract-function-preg-last-error-f10701376a area=builtin_contract kind=function symbol=preg_last_error source=ext/pcre/php_pcre.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-preg-last-error-f10701376a failure_category=builtin_contract requires_ref_extension=pcre
$name = "preg_last_error";
echo function_exists($name) ? "available\n" : "missing\n";
