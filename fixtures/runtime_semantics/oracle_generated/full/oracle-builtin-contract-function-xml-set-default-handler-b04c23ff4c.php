<?php
// oracle-probe: id=oracle-builtin-contract-function-xml-set-default-handler-b04c23ff4c area=builtin_contract kind=function symbol=xml_set_default_handler source=ext/xml/xml.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-xml-set-default-handler-b04c23ff4c failure_category=builtin_contract requires_ref_extension=xml
$name = "xml_set_default_handler";
echo function_exists($name) ? "available\n" : "missing\n";
