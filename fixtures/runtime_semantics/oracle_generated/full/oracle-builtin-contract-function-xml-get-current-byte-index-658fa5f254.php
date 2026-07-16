<?php
// oracle-probe: id=oracle-builtin-contract-function-xml-get-current-byte-index-658fa5f254 area=builtin_contract kind=function symbol=xml_get_current_byte_index source=ext/xml/xml.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-xml-get-current-byte-index-658fa5f254 failure_category=builtin_contract requires_ref_extension=xml
$name = "xml_get_current_byte_index";
echo function_exists($name) ? "available\n" : "missing\n";
