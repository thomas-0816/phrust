<?php
// oracle-probe: id=oracle-internal-api-contract-class-constant-splfileobject-drop-new-line-fad19d4e9f area=internal_api_contract kind=class_constant symbol=SplFileObject::DROP_NEW_LINE source=ext/spl/spl_directory.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-constant-splfileobject-drop-new-line-fad19d4e9f failure_category=internal_api_contract requires_ref_extension=spl
$class = "SplFileObject";
$member = "DROP_NEW_LINE";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && defined($class . '::' . $member);
echo $available ? "available\n" : "missing\n";
