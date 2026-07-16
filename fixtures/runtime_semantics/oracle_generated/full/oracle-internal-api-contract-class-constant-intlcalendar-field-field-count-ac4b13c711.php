<?php
// oracle-probe: id=oracle-internal-api-contract-class-constant-intlcalendar-field-field-count-ac4b13c711 area=internal_api_contract kind=class_constant symbol=IntlCalendar::FIELD_FIELD_COUNT source=ext/intl/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-class-constant-intlcalendar-field-field-count-ac4b13c711 failure_category=internal_api_contract requires_ref_extension=intl
$class = "IntlCalendar";
$member = "FIELD_FIELD_COUNT";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && defined($class . '::' . $member);
echo $available ? "available\n" : "missing\n";
