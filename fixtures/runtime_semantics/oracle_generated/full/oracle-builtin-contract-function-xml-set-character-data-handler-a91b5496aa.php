<?php
// oracle-probe: id=oracle-builtin-contract-function-xml-set-character-data-handler-a91b5496aa area=builtin_contract kind=function symbol=xml_set_character_data_handler source=ext/xml/xml.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-xml-set-character-data-handler-a91b5496aa failure_category=builtin_contract requires_ref_extension=xml
try {
    $result = \xml_set_character_data_handler();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
