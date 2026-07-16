<?php
// oracle-probe: id=oracle-internal-api-contract-property-exception-code-0ebda44e3d area=internal_api_contract kind=property symbol=Exception::code source=Zend/zend_exceptions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-internal-api-contract-property-exception-code-0ebda44e3d failure_category=internal_api_contract
$class = "Exception";
$member = "code";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && (new ReflectionClass($class))->hasProperty($member);
echo $available ? "available\n" : "missing\n";
