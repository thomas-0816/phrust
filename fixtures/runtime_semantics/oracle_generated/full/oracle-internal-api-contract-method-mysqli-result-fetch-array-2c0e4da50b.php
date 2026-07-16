<?php
// oracle-probe: id=oracle-internal-api-contract-method-mysqli-result-fetch-array-2c0e4da50b area=internal_api_contract kind=method symbol=mysqli_result::fetch_array source=ext/mysqli/mysqli.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-method-mysqli-result-fetch-array-2c0e4da50b failure_category=internal_api_contract requires_ref_extension=mysqli
$class = "mysqli_result";
$member = "fetch_array";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
