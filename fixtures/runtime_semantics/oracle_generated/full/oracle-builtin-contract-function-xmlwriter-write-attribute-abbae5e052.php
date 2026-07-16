<?php
// oracle-probe: id=oracle-builtin-contract-function-xmlwriter-write-attribute-abbae5e052 area=builtin_contract kind=function symbol=xmlwriter_write_attribute source=ext/xmlwriter/php_xmlwriter.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-xmlwriter-write-attribute-abbae5e052 failure_category=builtin_contract requires_ref_extension=xmlwriter
$name = "xmlwriter_write_attribute";
echo function_exists($name) ? "available\n" : "missing\n";
