<?php
// oracle-probe: id=oracle-internal-api-contract-method-parentiterator-next-8db21e3d18 area=internal_api_contract kind=method symbol=ParentIterator::next source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-method-parentiterator-next-8db21e3d18 failure_category=internal_api_contract requires_ref_extension=spl
$class = "ParentIterator";
$member = "next";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
