<?php
// oracle-probe: id=oracle-builtin-contract-function-bindtextdomain-9d9e58dd9a area=builtin_contract kind=function symbol=bindtextdomain source=ext/gettext/gettext.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-bindtextdomain-9d9e58dd9a failure_category=builtin_contract requires_ref_extension=gettext
$name = "bindtextdomain";
echo function_exists($name) ? "available\n" : "missing\n";
