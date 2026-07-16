<?php
// oracle-probe: id=oracle-builtin-contract-function-dgettext-9020fa04c9 area=builtin_contract kind=function symbol=dgettext source=ext/gettext/gettext.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-dgettext-9020fa04c9 failure_category=builtin_contract requires_ref_extension=gettext
try {
    $result = \dgettext();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
