<?php
// oracle-probe: id=oracle-builtin-contract-function-intl-get-error-code-5447d94497 area=builtin_contract kind=function symbol=intl_get_error_code source=ext/intl/php_intl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-intl-get-error-code-5447d94497 failure_category=builtin_contract requires_ref_extension=intl
$name = "intl_get_error_code";
echo function_exists($name) ? "available\n" : "missing\n";
