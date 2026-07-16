<?php
// oracle-probe: id=oracle-builtin-contract-function-ldap-exop-whoami-0e794d5fd7 area=builtin_contract kind=function symbol=ldap_exop_whoami source=ext/ldap/ldap.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ldap-exop-whoami-0e794d5fd7 failure_category=builtin_contract requires_ref_extension=ldap
$name = "ldap_exop_whoami";
echo function_exists($name) ? "available\n" : "missing\n";
