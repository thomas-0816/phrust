<?php
// oracle-probe: id=oracle-internal-api-contract-method-splfileobject-fflush-190a05d483 area=internal_api_contract kind=method symbol=SplFileObject::fflush source=ext/spl/spl_directory.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-method-splfileobject-fflush-190a05d483 failure_category=internal_api_contract requires_ref_extension=spl
$class = "SplFileObject";
$member = "fflush";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
