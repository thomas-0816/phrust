<?php
// oracle-probe: id=oracle-builtin-contract-function-grapheme-substr-e1359c4e69 area=builtin_contract kind=function symbol=grapheme_substr source=ext/intl/php_intl.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-grapheme-substr-e1359c4e69 failure_category=builtin_contract requires_ref_extension=intl
$name = "grapheme_substr";
echo function_exists($name) ? "available\n" : "missing\n";
