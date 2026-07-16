<?php
// oracle-probe: id=oracle-builtin-contract-function-xml-parse-into-struct-caaedb1b27 area=builtin_contract kind=function symbol=xml_parse_into_struct source=ext/xml/xml.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-xml-parse-into-struct-caaedb1b27 failure_category=builtin_contract requires_ref_extension=xml
$name = "xml_parse_into_struct";
echo function_exists($name) ? "available\n" : "missing\n";
