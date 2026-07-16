<?php
// oracle-probe: id=oracle-internal-api-contract-property-mysqli-stmt-sqlstate-9a8aff98f0 area=internal_api_contract kind=property symbol=mysqli_stmt::sqlstate source=ext/mysqli/mysqli.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-property-mysqli-stmt-sqlstate-9a8aff98f0 failure_category=internal_api_contract requires_ref_extension=mysqli
$class = "mysqli_stmt";
$member = "sqlstate";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && (new ReflectionClass($class))->hasProperty($member);
echo $available ? "available\n" : "missing\n";
