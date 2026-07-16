<?php
// oracle-probe: id=oracle-builtin-contract-function-xml-get-current-column-number-54455c7e46 area=builtin_contract kind=function symbol=xml_get_current_column_number source=ext/xml/xml.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-xml-get-current-column-number-54455c7e46 failure_category=builtin_contract requires_ref_extension=xml
try {
    $result = \xml_get_current_column_number();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
