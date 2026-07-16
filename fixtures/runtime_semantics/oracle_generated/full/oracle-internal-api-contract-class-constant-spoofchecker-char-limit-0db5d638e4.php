<?php
// oracle-probe: id=oracle-internal-api-contract-class-constant-spoofchecker-char-limit-0db5d638e4 area=internal_api_contract kind=class_constant symbol=Spoofchecker::CHAR_LIMIT source=ext/intl/spoofchecker/spoofchecker.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-constant-spoofchecker-char-limit-0db5d638e4 failure_category=internal_api_contract requires_ref_extension=intl
$class = "Spoofchecker";
$member = "CHAR_LIMIT";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && defined($class . '::' . $member);
echo $available ? "available\n" : "missing\n";
