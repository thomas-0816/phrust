<?php
// oracle-probe: id=oracle-internal-api-contract-class-recursivetreeiterator-1cfe3b0acd area=internal_api_contract kind=class symbol=RecursiveTreeIterator source=ext/spl/spl_iterators.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-recursivetreeiterator-1cfe3b0acd failure_category=internal_api_contract requires_ref_extension=spl
$class = "RecursiveTreeIterator";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
