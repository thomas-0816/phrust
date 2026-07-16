<?php
// oracle-probe: id=oracle-internal-api-contract-class-constant-pdo-attr-fetch-catalog-names-9ea8a1d7dd area=internal_api_contract kind=class_constant symbol=PDO::ATTR_FETCH_CATALOG_NAMES source=ext/pdo/pdo_dbh.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-constant-pdo-attr-fetch-catalog-names-9ea8a1d7dd failure_category=internal_api_contract requires_ref_extension=pdo
$class = "PDO";
$member = "ATTR_FETCH_CATALOG_NAMES";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && defined($class . '::' . $member);
echo $available ? "available\n" : "missing\n";
