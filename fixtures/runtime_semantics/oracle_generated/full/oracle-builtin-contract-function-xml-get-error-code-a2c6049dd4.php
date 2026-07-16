<?php
// oracle-probe: id=oracle-builtin-contract-function-xml-get-error-code-a2c6049dd4 area=builtin_contract kind=function symbol=xml_get_error_code source=ext/xml/xml.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-xml-get-error-code-a2c6049dd4 failure_category=builtin_contract requires_ref_extension=xml
try {
    $result = \xml_get_error_code();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
