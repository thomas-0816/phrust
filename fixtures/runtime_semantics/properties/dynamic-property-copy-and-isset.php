<?php

class PropertyCopyTarget
{
    public $first;
    public $second = '';
    public $third = 0;

    public function __construct($source)
    {
        foreach (get_object_vars($source) as $key => $value) {
            $this->$key = $value;
        }
    }
}

$source = new stdClass();
$source->first = 1;
$source->second = 'two';
$source->third = 3;

function identity_property_value($value)
{
    return $value;
}

$sanitized = clone $source;
foreach (array('first', 'second', 'third') as $field) {
    if (isset($sanitized->$field)) {
        $sanitized->$field = identity_property_value($sanitized->$field);
    }
}
echo json_encode(get_object_vars($sanitized)), "\n";

$copy = new PropertyCopyTarget($source);
echo json_encode(get_object_vars($copy)), "\n";

class ObjectCloneCache
{
    public $cache = array();

    public function set($key, $data)
    {
        if (is_object($data)) {
            $data = clone $data;
        }
        $this->cache[$key] = $data;
    }

    public function get($key)
    {
        return clone $this->cache[$key];
    }
}

$cache = new ObjectCloneCache();
$cached = clone $source;
unset($cached->missing);
$cache->set(1, $cached);
echo json_encode(get_object_vars($cache->get(1))), "\n";

$null = null;
$copy->second =& $null;
var_dump(isset($copy->second));
$null = 'set';
var_dump(isset($copy->second));

require __DIR__ . '/_data/declared-array-property-copy.php';
$array_property = new DeclaredArrayPropertyCopy(array('name' => 'original'));
echo json_encode($array_property->mutateCopy()), "\n";
$array_property = new DeclaredArrayPropertyCopy(array('name' => 'original'));
echo json_encode(mutate_declared_array_property_copy($array_property)), "\n";
$nested_property = new NestedDeclaredArrayPropertyCopy(array(
    'name' => 'original',
    'children' => array(array('name' => 'child')),
));
echo json_encode(mutate_declared_array_property_copy($nested_property)), "\n";
$parsed_blocks = array(array('name' => 'original'));
$array_property = new DeclaredArrayPropertyCopy($parsed_blocks[0]);
echo json_encode(mutate_declared_array_property_copy($array_property)), "\n";
$block_source = new stdClass();
$block_source->blocks = $parsed_blocks;
$array_property = new DeclaredArrayPropertyCopy($block_source->blocks[0]);
echo json_encode(mutate_declared_array_property_copy($array_property)), "\n";
