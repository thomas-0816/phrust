<?php
// oracle-probe: id=oracle-builtin-contract-function-preg-replace-98765f1e45 area=builtin_contract kind=function symbol=preg_replace source=ext/pcre/php_pcre.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-preg-replace-98765f1e45 failure_category=builtin_contract requires_ref_extension=pcre
$name = "preg_replace";
echo function_exists($name) ? "available\n" : "missing\n";
