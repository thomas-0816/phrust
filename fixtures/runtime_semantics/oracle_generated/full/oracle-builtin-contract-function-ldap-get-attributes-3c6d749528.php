<?php
// oracle-probe: id=oracle-builtin-contract-function-ldap-get-attributes-3c6d749528 area=builtin_contract kind=function symbol=ldap_get_attributes source=ext/ldap/ldap.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-ldap-get-attributes-3c6d749528 failure_category=builtin_contract requires_ref_extension=ldap
try {
    $result = \ldap_get_attributes();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
