<?php
// oracle-probe: id=oracle-internal-api-contract-class-unexpectedvalueexception-7dfc211632 area=internal_api_contract kind=class symbol=UnexpectedValueException source=ext/spl/spl_exceptions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-unexpectedvalueexception-7dfc211632 failure_category=internal_api_contract requires_ref_extension=spl
$class = "UnexpectedValueException";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
