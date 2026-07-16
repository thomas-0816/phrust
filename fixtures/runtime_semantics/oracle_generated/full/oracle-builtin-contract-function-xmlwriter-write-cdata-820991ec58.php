<?php
// oracle-probe: id=oracle-builtin-contract-function-xmlwriter-write-cdata-820991ec58 area=builtin_contract kind=function symbol=xmlwriter_write_cdata source=ext/xmlwriter/php_xmlwriter.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-xmlwriter-write-cdata-820991ec58 failure_category=builtin_contract requires_ref_extension=xmlwriter
try {
    $result = \xmlwriter_write_cdata();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
