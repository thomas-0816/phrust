<?php
// oracle-probe: id=oracle-builtin-contract-function-pdo-drivers-358c65ad0d area=builtin_contract kind=function symbol=pdo_drivers source=ext/pdo/pdo.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-pdo-drivers-358c65ad0d failure_category=builtin_contract requires_ref_extension=pdo
$name = "pdo_drivers";
echo function_exists($name) ? "available\n" : "missing\n";
