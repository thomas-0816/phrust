<?php
// oracle-probe: id=oracle-internal-api-contract-class-constant-intlpartsiterator-key-right-f591c88d9e area=internal_api_contract kind=class_constant symbol=IntlPartsIterator::KEY_RIGHT source=ext/intl/breakiterator/breakiterator_iterators.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-constant-intlpartsiterator-key-right-f591c88d9e failure_category=internal_api_contract requires_ref_extension=intl
$class = "IntlPartsIterator";
$member = "KEY_RIGHT";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && defined($class . '::' . $member);
echo $available ? "available\n" : "missing\n";
