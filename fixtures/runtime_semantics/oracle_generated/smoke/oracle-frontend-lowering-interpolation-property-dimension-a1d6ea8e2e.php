<?php
// oracle-probe: id=oracle-frontend-lowering-interpolation-property-dimension-a1d6ea8e2e area=frontend_lowering kind=interpolation symbol=property-dimension source=seed expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-frontend-lowering-interpolation-property-dimension-a1d6ea8e2e failure_category=frontend_lowering
class OracleInterpolationBox { public array $items = ["k" => "v"]; }
$box = new OracleInterpolationBox();
echo "{$box->items['k']}\n";
