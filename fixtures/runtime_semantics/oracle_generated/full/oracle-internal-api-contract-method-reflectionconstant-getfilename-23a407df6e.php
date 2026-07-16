<?php
// oracle-probe: id=oracle-internal-api-contract-method-reflectionconstant-getfilename-23a407df6e area=internal_api_contract kind=method symbol=ReflectionConstant::getFileName source=ext/reflection/php_reflection.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-method-reflectionconstant-getfilename-23a407df6e failure_category=internal_api_contract requires_ref_extension=reflection
$class = "ReflectionConstant";
$member = "getFileName";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
