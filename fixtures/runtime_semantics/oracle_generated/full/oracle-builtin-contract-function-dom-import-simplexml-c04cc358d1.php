<?php
// oracle-probe: id=oracle-builtin-contract-function-dom-import-simplexml-c04cc358d1 area=builtin_contract kind=function symbol=dom_import_simplexml source=ext/dom/php_dom.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-dom-import-simplexml-c04cc358d1 failure_category=builtin_contract requires_ref_extension=dom
$name = "dom_import_simplexml";
echo function_exists($name) ? "available\n" : "missing\n";
