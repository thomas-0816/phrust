<?php
// oracle-probe: id=oracle-builtin-contract-function-xml-parser-create-ns-e0e7dc0532 area=builtin_contract kind=function symbol=xml_parser_create_ns source=ext/xml/xml.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-xml-parser-create-ns-e0e7dc0532 failure_category=builtin_contract requires_ref_extension=xml
try {
    $result = \xml_parser_create_ns(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
