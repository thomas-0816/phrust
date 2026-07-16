<?php
// oracle-probe: id=oracle-builtin-contract-function-preg-grep-ad802c1098 area=builtin_contract kind=function symbol=preg_grep source=ext/pcre/php_pcre.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-preg-grep-ad802c1098 failure_category=builtin_contract requires_ref_extension=pcre
$name = "preg_grep";
echo function_exists($name) ? "available\n" : "missing\n";
