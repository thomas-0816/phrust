<?php
// oracle-probe: id=oracle-internal-api-contract-class-constant-pdo-pgsql-transaction-intrans-541c41ac2b area=internal_api_contract kind=class_constant symbol=Pdo\Pgsql::TRANSACTION_INTRANS source=ext/pdo_pgsql/pdo_pgsql.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-constant-pdo-pgsql-transaction-intrans-541c41ac2b failure_category=internal_api_contract requires_ref_extension=pdo_pgsql
$class = "Pdo\\Pgsql";
$member = "TRANSACTION_INTRANS";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && defined($class . '::' . $member);
echo $available ? "available\n" : "missing\n";
