<?php
// oracle-probe: id=oracle-internal-api-contract-class-recursiveiteratoriterator-556cb31a6c area=internal_api_contract kind=class symbol=RecursiveIteratorIterator source=ext/spl/spl_iterators.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-recursiveiteratoriterator-556cb31a6c failure_category=internal_api_contract requires_ref_extension=spl
$class = "RecursiveIteratorIterator";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
