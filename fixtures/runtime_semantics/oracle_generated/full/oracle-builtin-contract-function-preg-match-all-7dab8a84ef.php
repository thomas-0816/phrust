<?php
// oracle-probe: id=oracle-builtin-contract-function-preg-match-all-7dab8a84ef area=builtin_contract kind=function symbol=preg_match_all source=ext/pcre/php_pcre.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-preg-match-all-7dab8a84ef failure_category=builtin_contract requires_ref_extension=pcre
$name = "preg_match_all";
echo function_exists($name) ? "available\n" : "missing\n";
