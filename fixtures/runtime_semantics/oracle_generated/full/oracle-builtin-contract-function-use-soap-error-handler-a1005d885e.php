<?php
// oracle-probe: id=oracle-builtin-contract-function-use-soap-error-handler-a1005d885e area=builtin_contract kind=function symbol=use_soap_error_handler source=ext/soap/soap.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-use-soap-error-handler-a1005d885e failure_category=builtin_contract requires_ref_extension=soap
$name = "use_soap_error_handler";
echo function_exists($name) ? "available\n" : "missing\n";
