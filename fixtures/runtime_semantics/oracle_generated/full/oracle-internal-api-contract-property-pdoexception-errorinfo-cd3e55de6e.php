<?php
// oracle-probe: id=oracle-internal-api-contract-property-pdoexception-errorinfo-cd3e55de6e area=internal_api_contract kind=property symbol=PDOException::errorInfo source=ext/pdo/pdo.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-property-pdoexception-errorinfo-cd3e55de6e failure_category=internal_api_contract requires_ref_extension=pdo
$class = "PDOException";
$member = "errorInfo";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && (new ReflectionClass($class))->hasProperty($member);
echo $available ? "available\n" : "missing\n";
