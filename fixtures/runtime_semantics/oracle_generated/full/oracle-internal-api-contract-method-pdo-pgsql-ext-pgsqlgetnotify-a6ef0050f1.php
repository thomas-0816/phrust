<?php
// oracle-probe: id=oracle-internal-api-contract-method-pdo-pgsql-ext-pgsqlgetnotify-a6ef0050f1 area=internal_api_contract kind=method symbol=PDO_PGSql_Ext::pgsqlGetNotify source=ext/pdo_pgsql/pgsql_driver.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-method-pdo-pgsql-ext-pgsqlgetnotify-a6ef0050f1 failure_category=internal_api_contract requires_ref_extension=pdo_pgsql
$class = "PDO_PGSql_Ext";
$member = "pgsqlGetNotify";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
