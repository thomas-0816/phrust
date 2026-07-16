<?php
// oracle-probe: id=oracle-builtin-contract-function-preg-filter-edc7d1aaeb area=builtin_contract kind=function symbol=preg_filter source=ext/pcre/php_pcre.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-preg-filter-edc7d1aaeb failure_category=builtin_contract requires_ref_extension=pcre
$name = "preg_filter";
echo function_exists($name) ? "available\n" : "missing\n";
