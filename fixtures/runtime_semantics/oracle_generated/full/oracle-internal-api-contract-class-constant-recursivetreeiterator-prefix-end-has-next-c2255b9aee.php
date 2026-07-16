<?php
// oracle-probe: id=oracle-internal-api-contract-class-constant-recursivetreeiterator-prefix-end-has-next-c2255b9aee area=internal_api_contract kind=class_constant symbol=RecursiveTreeIterator::PREFIX_END_HAS_NEXT source=ext/spl/spl_iterators.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-constant-recursivetreeiterator-prefix-end-has-next-c2255b9aee failure_category=internal_api_contract requires_ref_extension=spl
$class = "RecursiveTreeIterator";
$member = "PREFIX_END_HAS_NEXT";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && defined($class . '::' . $member);
echo $available ? "available\n" : "missing\n";
