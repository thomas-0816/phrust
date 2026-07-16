<?php
// oracle-probe: id=oracle-internal-api-contract-method-sqlite3result-finalize-9aeaa54c1e area=internal_api_contract kind=method symbol=SQLite3Result::finalize source=ext/sqlite3/sqlite3.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-method-sqlite3result-finalize-9aeaa54c1e failure_category=internal_api_contract requires_ref_extension=sqlite3
$class = "SQLite3Result";
$member = "finalize";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
