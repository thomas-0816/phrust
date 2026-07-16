<?php
// oracle-probe: id=oracle-reference-binding-reference-object-property-24d7021f76 area=reference_binding kind=reference symbol=object-property source=seed expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-reference-binding-reference-object-property-24d7021f76 failure_category=reference_binding
class OracleReferenceBox { public int $value = 1; }
$box = new OracleReferenceBox();
$alias =& $box->value;
$alias = 8;
echo $box->value, "\n";
