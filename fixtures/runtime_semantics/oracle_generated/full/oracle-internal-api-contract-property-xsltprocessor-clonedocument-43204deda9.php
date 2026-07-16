<?php
// oracle-probe: id=oracle-internal-api-contract-property-xsltprocessor-clonedocument-43204deda9 area=internal_api_contract kind=property symbol=XSLTProcessor::cloneDocument source=ext/xsl/php_xsl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-property-xsltprocessor-clonedocument-43204deda9 failure_category=internal_api_contract requires_ref_extension=xsl
$class = "XSLTProcessor";
$member = "cloneDocument";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && (new ReflectionClass($class))->hasProperty($member);
echo $available ? "available\n" : "missing\n";
