<?php
// oracle-probe: id=oracle-builtin-contract-function-locale-get-primary-language-86c9e1f867 area=builtin_contract kind=function symbol=locale_get_primary_language source=ext/intl/php_intl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-locale-get-primary-language-86c9e1f867 failure_category=builtin_contract requires_ref_extension=intl
try {
    $result = \locale_get_primary_language();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
