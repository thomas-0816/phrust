<?php
// oracle-probe: id=oracle-builtin-contract-function-preg-replace-callback-63200a294f area=builtin_contract kind=function symbol=preg_replace_callback source=ext/pcre/php_pcre.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-preg-replace-callback-63200a294f failure_category=builtin_contract requires_ref_extension=pcre
$name = "preg_replace_callback";
echo function_exists($name) ? "available\n" : "missing\n";
