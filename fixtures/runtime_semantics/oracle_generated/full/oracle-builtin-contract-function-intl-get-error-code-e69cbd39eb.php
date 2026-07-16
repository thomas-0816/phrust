<?php
// oracle-probe: id=oracle-builtin-contract-function-intl-get-error-code-e69cbd39eb area=builtin_contract kind=function symbol=intl_get_error_code source=ext/intl/php_intl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-intl-get-error-code-e69cbd39eb failure_category=builtin_contract requires_ref_extension=intl
try {
    $result = \intl_get_error_code(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
