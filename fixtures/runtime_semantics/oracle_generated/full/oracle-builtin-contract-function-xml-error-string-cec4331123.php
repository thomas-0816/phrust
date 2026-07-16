<?php
// oracle-probe: id=oracle-builtin-contract-function-xml-error-string-cec4331123 area=builtin_contract kind=function symbol=xml_error_string source=ext/xml/xml.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-xml-error-string-cec4331123 failure_category=builtin_contract requires_ref_extension=xml
try {
    $result = \xml_error_string();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
