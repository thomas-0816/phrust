<?php
// oracle-probe: id=oracle-internal-api-contract-property-domdocument-xmlversion-2cbec1c0fd area=internal_api_contract kind=property symbol=DOMDocument::xmlVersion source=ext/dom/php_dom.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-property-domdocument-xmlversion-2cbec1c0fd failure_category=internal_api_contract requires_ref_extension=dom
$class = "DOMDocument";
$member = "xmlVersion";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && (new ReflectionClass($class))->hasProperty($member);
echo $available ? "available\n" : "missing\n";
