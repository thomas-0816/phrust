<?php
// oracle-probe: id=oracle-internal-api-contract-class-constant-globiterator-current-as-self-f9b458bd96 area=internal_api_contract kind=class_constant symbol=GlobIterator::CURRENT_AS_SELF source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-constant-globiterator-current-as-self-f9b458bd96 failure_category=internal_api_contract requires_ref_extension=spl
$class = "GlobIterator";
$member = "CURRENT_AS_SELF";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && defined($class . '::' . $member);
echo $available ? "available\n" : "missing\n";
