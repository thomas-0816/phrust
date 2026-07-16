<?php
// oracle-probe: id=oracle-builtin-contract-function-get-html-translation-table-519df58221 area=builtin_contract kind=function symbol=get_html_translation_table source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-html-translation-table-519df58221 failure_category=builtin_contract
try {
    $result = \get_html_translation_table(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
