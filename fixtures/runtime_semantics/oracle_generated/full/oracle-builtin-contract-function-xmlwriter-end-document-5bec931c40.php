<?php
// oracle-probe: id=oracle-builtin-contract-function-xmlwriter-end-document-5bec931c40 area=builtin_contract kind=function symbol=xmlwriter_end_document source=ext/xmlwriter/php_xmlwriter.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-xmlwriter-end-document-5bec931c40 failure_category=builtin_contract requires_ref_extension=xmlwriter
$name = "xmlwriter_end_document";
echo function_exists($name) ? "available\n" : "missing\n";
