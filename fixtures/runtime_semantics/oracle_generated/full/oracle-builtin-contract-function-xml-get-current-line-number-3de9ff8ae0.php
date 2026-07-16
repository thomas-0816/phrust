<?php
// oracle-probe: id=oracle-builtin-contract-function-xml-get-current-line-number-3de9ff8ae0 area=builtin_contract kind=function symbol=xml_get_current_line_number source=ext/xml/xml.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-xml-get-current-line-number-3de9ff8ae0 failure_category=builtin_contract requires_ref_extension=xml
$name = "xml_get_current_line_number";
echo function_exists($name) ? "available\n" : "missing\n";
