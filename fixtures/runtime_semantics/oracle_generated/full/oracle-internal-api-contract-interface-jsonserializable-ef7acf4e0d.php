<?php
// oracle-probe: id=oracle-internal-api-contract-interface-jsonserializable-ef7acf4e0d area=internal_api_contract kind=interface symbol=JsonSerializable source=ext/json/json.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-interface-jsonserializable-ef7acf4e0d failure_category=internal_api_contract requires_ref_extension=json
$class = "JsonSerializable";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
