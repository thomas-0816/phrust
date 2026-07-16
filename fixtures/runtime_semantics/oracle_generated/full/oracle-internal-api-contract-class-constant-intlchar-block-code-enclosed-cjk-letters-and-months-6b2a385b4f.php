<?php
// oracle-probe: id=oracle-internal-api-contract-class-constant-intlchar-block-code-enclosed-cjk-letters-and-months-6b2a385b4f area=internal_api_contract kind=class_constant symbol=IntlChar::BLOCK_CODE_ENCLOSED_CJK_LETTERS_AND_MONTHS source=ext/intl/uchar/uchar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-constant-intlchar-block-code-enclosed-cjk-letters-and-months-6b2a385b4f failure_category=internal_api_contract requires_ref_extension=intl
$class = "IntlChar";
$member = "BLOCK_CODE_ENCLOSED_CJK_LETTERS_AND_MONTHS";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && defined($class . '::' . $member);
echo $available ? "available\n" : "missing\n";
