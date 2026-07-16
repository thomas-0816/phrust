<?php
// oracle-probe: id=oracle-internal-api-contract-method-splfixedarray-setsize-2ae49f9d90 area=internal_api_contract kind=method symbol=SplFixedArray::setSize source=ext/spl/spl_fixedarray.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-method-splfixedarray-setsize-2ae49f9d90 failure_category=internal_api_contract requires_ref_extension=spl
$class = "SplFixedArray";
$member = "setSize";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
