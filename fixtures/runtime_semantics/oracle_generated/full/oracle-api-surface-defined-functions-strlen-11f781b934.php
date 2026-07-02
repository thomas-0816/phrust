<?php
// oracle-probe: id=oracle-api-surface-defined-functions-strlen-11f781b934 area=api_surface kind=defined-functions symbol=strlen source=reference-php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-api-surface-defined-functions-strlen-11f781b934 failure_category=api_surface
$functions = get_defined_functions()["internal"];
echo in_array("strlen", $functions, true) ? "listed\n" : "missing\n";
