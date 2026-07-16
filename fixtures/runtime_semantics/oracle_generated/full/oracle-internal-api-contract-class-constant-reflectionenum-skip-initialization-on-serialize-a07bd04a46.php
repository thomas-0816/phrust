<?php
// oracle-probe: id=oracle-internal-api-contract-class-constant-reflectionenum-skip-initialization-on-serialize-a07bd04a46 area=internal_api_contract kind=class_constant symbol=ReflectionEnum::SKIP_INITIALIZATION_ON_SERIALIZE source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-constant-reflectionenum-skip-initialization-on-serialize-a07bd04a46 failure_category=internal_api_contract requires_ref_extension=reflection
$class = "ReflectionEnum";
$member = "SKIP_INITIALIZATION_ON_SERIALIZE";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && defined($class . '::' . $member);
echo $available ? "available\n" : "missing\n";
