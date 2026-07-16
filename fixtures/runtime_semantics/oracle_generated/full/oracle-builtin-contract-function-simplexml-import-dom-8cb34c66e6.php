<?php
// oracle-probe: id=oracle-builtin-contract-function-simplexml-import-dom-8cb34c66e6 area=builtin_contract kind=function symbol=simplexml_import_dom source=ext/simplexml/simplexml.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-simplexml-import-dom-8cb34c66e6 failure_category=builtin_contract requires_ref_extension=simplexml
$name = "simplexml_import_dom";
echo function_exists($name) ? "available\n" : "missing\n";
