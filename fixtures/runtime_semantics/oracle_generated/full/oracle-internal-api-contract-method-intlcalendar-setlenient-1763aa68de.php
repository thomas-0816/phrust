<?php
// oracle-probe: id=oracle-internal-api-contract-method-intlcalendar-setlenient-1763aa68de area=internal_api_contract kind=method symbol=IntlCalendar::setLenient source=ext/intl/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-internal-api-contract-method-intlcalendar-setlenient-1763aa68de failure_category=internal_api_contract requires_ref_extension=intl
$class = "IntlCalendar";
$member = "setLenient";
$classAvailable = class_exists($class, false) || interface_exists($class, false) || trait_exists($class, false) || (function_exists('enum_exists') && enum_exists($class, false));
$available = $classAvailable && method_exists($class, $member);
echo $available ? "available\n" : "missing\n";
