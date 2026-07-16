<?php
// oracle-probe: id=oracle-internal-api-contract-class-constant-pdo-param-evt-alloc-ca00077b4b area=internal_api_contract kind=class_constant symbol=PDO::PARAM_EVT_ALLOC source=ext/pdo/pdo_dbh.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-constant-pdo-param-evt-alloc-ca00077b4b failure_category=internal_api_contract requires_ref_extension=pdo
$class = "PDO";
$member = "PARAM_EVT_ALLOC";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && defined($class . '::' . $member);
echo $available ? "available\n" : "missing\n";
