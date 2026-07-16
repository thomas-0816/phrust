<?php
// oracle-probe: id=oracle-builtin-contract-function-ldap-mod-add-8d616ed989 area=builtin_contract kind=function symbol=ldap_mod_add source=ext/ldap/ldap.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ldap-mod-add-8d616ed989 failure_category=builtin_contract requires_ref_extension=ldap
try {
    $result = \ldap_mod_add();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
