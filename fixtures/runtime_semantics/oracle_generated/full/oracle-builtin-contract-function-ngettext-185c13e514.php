<?php
// oracle-probe: id=oracle-builtin-contract-function-ngettext-185c13e514 area=builtin_contract kind=function symbol=ngettext source=ext/gettext/gettext.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ngettext-185c13e514 failure_category=builtin_contract requires_ref_extension=gettext
$name = "ngettext";
echo function_exists($name) ? "available\n" : "missing\n";
