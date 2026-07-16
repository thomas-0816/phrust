<?php
// oracle-probe: id=oracle-internal-api-contract-class-spoofchecker-b022abb55d area=internal_api_contract kind=class symbol=Spoofchecker source=ext/intl/spoofchecker/spoofchecker.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-spoofchecker-b022abb55d failure_category=internal_api_contract requires_ref_extension=intl
$class = "Spoofchecker";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
