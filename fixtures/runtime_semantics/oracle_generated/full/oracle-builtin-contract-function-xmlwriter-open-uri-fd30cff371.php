<?php
// oracle-probe: id=oracle-builtin-contract-function-xmlwriter-open-uri-fd30cff371 area=builtin_contract kind=function symbol=xmlwriter_open_uri source=ext/xmlwriter/php_xmlwriter.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-xmlwriter-open-uri-fd30cff371 failure_category=builtin_contract requires_ref_extension=xmlwriter
$name = "xmlwriter_open_uri";
echo function_exists($name) ? "available\n" : "missing\n";
