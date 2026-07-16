<?php
// oracle-probe: id=oracle-builtin-contract-function-gettext-4953d343d9 area=builtin_contract kind=function symbol=gettext source=ext/gettext/gettext.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gettext-4953d343d9 failure_category=builtin_contract requires_ref_extension=gettext
$name = "gettext";
echo function_exists($name) ? "available\n" : "missing\n";
