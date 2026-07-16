<?php
// oracle-probe: id=oracle-builtin-contract-function-date-create-immutable-from-format-16555fc193 area=builtin_contract kind=function symbol=date_create_immutable_from_format source=ext/date/php_date.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-date-create-immutable-from-format-16555fc193 failure_category=builtin_contract requires_ref_extension=date
$name = "date_create_immutable_from_format";
echo function_exists($name) ? "available\n" : "missing\n";
