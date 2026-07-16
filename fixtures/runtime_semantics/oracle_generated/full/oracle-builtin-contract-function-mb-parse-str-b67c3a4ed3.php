<?php
// oracle-probe: id=oracle-builtin-contract-function-mb-parse-str-b67c3a4ed3 area=builtin_contract kind=function symbol=mb_parse_str source=ext/mbstring/mbstring.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-mb-parse-str-b67c3a4ed3 failure_category=builtin_contract requires_ref_extension=mbstring
$name = "mb_parse_str";
echo function_exists($name) ? "available\n" : "missing\n";
