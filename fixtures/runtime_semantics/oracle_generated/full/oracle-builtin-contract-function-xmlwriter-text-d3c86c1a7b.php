<?php
// oracle-probe: id=oracle-builtin-contract-function-xmlwriter-text-d3c86c1a7b area=builtin_contract kind=function symbol=xmlwriter_text source=ext/xmlwriter/php_xmlwriter.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-xmlwriter-text-d3c86c1a7b failure_category=builtin_contract requires_ref_extension=xmlwriter
$name = "xmlwriter_text";
echo function_exists($name) ? "available\n" : "missing\n";
