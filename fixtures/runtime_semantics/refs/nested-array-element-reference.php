<?php
$parsed_block = array(
    'attrs' => array(
        'style' => array(
            'typography' => array('fontStyle' => 'var:preset|font-style|italic'),
        ),
    ),
);
$style_attribute = 'fontStyle';

$attribute_value =& $parsed_block['attrs']['style']['typography'][$style_attribute];
$attribute_value = 'italic';

echo $parsed_block['attrs']['style']['typography']['fontStyle'], "\n";
