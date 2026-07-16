<?php
// oracle-probe: id=oracle-builtin-contract-function-ldap-get-dn-2f4f19866e area=builtin_contract kind=function symbol=ldap_get_dn source=ext/ldap/ldap.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ldap-get-dn-2f4f19866e failure_category=builtin_contract requires_ref_extension=ldap
$name = "ldap_get_dn";
echo function_exists($name) ? "available\n" : "missing\n";
