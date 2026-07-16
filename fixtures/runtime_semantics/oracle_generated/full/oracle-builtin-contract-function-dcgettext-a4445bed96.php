<?php
// oracle-probe: id=oracle-builtin-contract-function-dcgettext-a4445bed96 area=builtin_contract kind=function symbol=dcgettext source=ext/gettext/gettext.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-dcgettext-a4445bed96 failure_category=builtin_contract requires_ref_extension=gettext
try {
    $result = \dcgettext();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
