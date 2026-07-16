<?php
// oracle-probe: id=oracle-internal-api-contract-class-constant-domnode-document-position-implementation-specific-2c4025d518 area=internal_api_contract kind=class_constant symbol=DOMNode::DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC source=ext/dom/php_dom.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-constant-domnode-document-position-implementation-specific-2c4025d518 failure_category=internal_api_contract requires_ref_extension=dom
$class = "DOMNode";
$member = "DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && defined($class . '::' . $member);
echo $available ? "available\n" : "missing\n";
