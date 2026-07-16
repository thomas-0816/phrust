<?php
// oracle-probe: id=oracle-builtin-contract-function-intl-get-error-message-14c1adec8e area=builtin_contract kind=function symbol=intl_get_error_message source=ext/intl/php_intl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-intl-get-error-message-14c1adec8e failure_category=builtin_contract requires_ref_extension=intl
$name = "intl_get_error_message";
echo function_exists($name) ? "available\n" : "missing\n";
