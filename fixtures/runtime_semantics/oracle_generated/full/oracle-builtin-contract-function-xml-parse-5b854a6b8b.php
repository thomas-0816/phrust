<?php
// oracle-probe: id=oracle-builtin-contract-function-xml-parse-5b854a6b8b area=builtin_contract kind=function symbol=xml_parse source=ext/xml/xml.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-xml-parse-5b854a6b8b failure_category=builtin_contract requires_ref_extension=xml
try {
    $result = \xml_parse();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
