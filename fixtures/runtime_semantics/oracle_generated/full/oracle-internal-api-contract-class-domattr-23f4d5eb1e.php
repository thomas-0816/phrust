<?php
// oracle-probe: id=oracle-internal-api-contract-class-domattr-23f4d5eb1e area=internal_api_contract kind=class symbol=DOMAttr source=ext/dom/php_dom.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-domattr-23f4d5eb1e failure_category=internal_api_contract requires_ref_extension=dom
$class = "DOMAttr";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
