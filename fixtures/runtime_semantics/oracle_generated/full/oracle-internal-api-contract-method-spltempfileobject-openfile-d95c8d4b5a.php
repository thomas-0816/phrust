<?php
// oracle-probe: id=oracle-internal-api-contract-method-spltempfileobject-openfile-d95c8d4b5a area=internal_api_contract kind=method symbol=SplTempFileObject::openFile source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-method-spltempfileobject-openfile-d95c8d4b5a failure_category=internal_api_contract requires_ref_extension=spl
$class = "SplTempFileObject";
$member = "openFile";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
