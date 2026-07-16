<?php
// oracle-probe: id=oracle-internal-api-contract-class-constant-recursiveregexiterator-match-8c52723214 area=internal_api_contract kind=class_constant symbol=RecursiveRegexIterator::MATCH source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-constant-recursiveregexiterator-match-8c52723214 failure_category=internal_api_contract requires_ref_extension=spl
$class = "RecursiveRegexIterator";
$member = "MATCH";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && defined($class . '::' . $member);
echo $available ? "available\n" : "missing\n";
