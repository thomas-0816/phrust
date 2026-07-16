<?php
// oracle-probe: id=oracle-builtin-contract-function-ldap-next-entry-00cc6175c9 area=builtin_contract kind=function symbol=ldap_next_entry source=ext/ldap/ldap.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ldap-next-entry-00cc6175c9 failure_category=builtin_contract requires_ref_extension=ldap
$name = "ldap_next_entry";
echo function_exists($name) ? "available\n" : "missing\n";
