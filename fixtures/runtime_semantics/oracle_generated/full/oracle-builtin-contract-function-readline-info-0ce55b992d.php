<?php
// oracle-probe: id=oracle-builtin-contract-function-readline-info-0ce55b992d area=builtin_contract kind=function symbol=readline_info source=ext/readline/readline.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-readline-info-0ce55b992d failure_category=builtin_contract requires_ref_extension=readline
$name = "readline_info";
echo function_exists($name) ? "available\n" : "missing\n";
