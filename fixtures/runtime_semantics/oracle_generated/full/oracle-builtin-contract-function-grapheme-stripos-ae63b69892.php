<?php
// oracle-probe: id=oracle-builtin-contract-function-grapheme-stripos-ae63b69892 area=builtin_contract kind=function symbol=grapheme_stripos source=ext/intl/php_intl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-grapheme-stripos-ae63b69892 failure_category=builtin_contract requires_ref_extension=intl
$name = "grapheme_stripos";
echo function_exists($name) ? "available\n" : "missing\n";
