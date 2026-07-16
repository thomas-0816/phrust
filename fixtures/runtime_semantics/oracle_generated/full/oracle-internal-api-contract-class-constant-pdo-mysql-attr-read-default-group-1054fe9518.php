<?php
// oracle-probe: id=oracle-internal-api-contract-class-constant-pdo-mysql-attr-read-default-group-1054fe9518 area=internal_api_contract kind=class_constant symbol=Pdo\Mysql::ATTR_READ_DEFAULT_GROUP source=ext/pdo_mysql/pdo_mysql.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-constant-pdo-mysql-attr-read-default-group-1054fe9518 failure_category=internal_api_contract requires_ref_extension=pdo_mysql
$class = "Pdo\\Mysql";
$member = "ATTR_READ_DEFAULT_GROUP";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && defined($class . '::' . $member);
echo $available ? "available\n" : "missing\n";
