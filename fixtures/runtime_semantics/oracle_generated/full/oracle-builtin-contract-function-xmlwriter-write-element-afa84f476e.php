<?php
// oracle-probe: id=oracle-builtin-contract-function-xmlwriter-write-element-afa84f476e area=builtin_contract kind=function symbol=xmlwriter_write_element source=ext/xmlwriter/php_xmlwriter.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-xmlwriter-write-element-afa84f476e failure_category=builtin_contract requires_ref_extension=xmlwriter
try {
    $result = \xmlwriter_write_element();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
