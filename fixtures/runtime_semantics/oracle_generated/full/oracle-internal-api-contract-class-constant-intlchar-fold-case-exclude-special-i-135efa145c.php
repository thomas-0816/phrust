<?php
// oracle-probe: id=oracle-internal-api-contract-class-constant-intlchar-fold-case-exclude-special-i-135efa145c area=internal_api_contract kind=class_constant symbol=IntlChar::FOLD_CASE_EXCLUDE_SPECIAL_I source=ext/intl/uchar/uchar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-constant-intlchar-fold-case-exclude-special-i-135efa145c failure_category=internal_api_contract requires_ref_extension=intl
$class = "IntlChar";
$member = "FOLD_CASE_EXCLUDE_SPECIAL_I";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && defined($class . '::' . $member);
echo $available ? "available\n" : "missing\n";
