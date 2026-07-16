<?php
// oracle-probe: id=oracle-builtin-contract-function-ldap-set-rebind-proc-91f4dd65dc area=builtin_contract kind=function symbol=ldap_set_rebind_proc source=ext/ldap/ldap.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ldap-set-rebind-proc-91f4dd65dc failure_category=builtin_contract requires_ref_extension=ldap
$name = "ldap_set_rebind_proc";
echo function_exists($name) ? "available\n" : "missing\n";
