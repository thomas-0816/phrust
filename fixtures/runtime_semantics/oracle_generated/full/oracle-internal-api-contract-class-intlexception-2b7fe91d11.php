<?php
// oracle-probe: id=oracle-internal-api-contract-class-intlexception-2b7fe91d11 area=internal_api_contract kind=class symbol=IntlException source=ext/intl/php_intl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-intlexception-2b7fe91d11 failure_category=internal_api_contract requires_ref_extension=intl
$class = "IntlException";
$available = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
echo $available ? "available\n" : "missing\n";
