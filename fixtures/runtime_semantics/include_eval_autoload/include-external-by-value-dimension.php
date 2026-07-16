<?php
// runtime-semantics: expect=pass

class ExternalByValueDimensionHolder
{
    public array $parsed = array('blockName' => 'original');
}

$holder = new ExternalByValueDimensionHolder();
include __DIR__ . '/_data/external-by-value-dimension.php';

$observed = external_by_value_dimension($holder->parsed['blockName']);
$copy = $holder->parsed;
$copy['blockName'] = 'copy';

echo json_encode(array(
    'observed' => $observed,
    'original' => $holder->parsed['blockName'],
    'copy' => $copy['blockName'],
)), "\n";
