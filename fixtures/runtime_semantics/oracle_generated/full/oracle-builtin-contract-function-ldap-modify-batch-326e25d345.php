<?php
// oracle-probe: id=oracle-builtin-contract-function-ldap-modify-batch-326e25d345 area=builtin_contract kind=function symbol=ldap_modify_batch source=ext/ldap/ldap.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ldap-modify-batch-326e25d345 failure_category=builtin_contract requires_ref_extension=ldap
$name = "ldap_modify_batch";
echo function_exists($name) ? "available\n" : "missing\n";
