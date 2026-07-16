<?php

class ReferenceSourceHolder
{
    public $value;
}

class ReferenceStaticCopyTarget
{
    public static $value;
}

class ReferenceValueRegistry
{
    public function getRegistered($name)
    {
        return $name;
    }
}

$source = new ReferenceSourceHolder();
$source->value = array('blockName' => 'source', 'attrs' => array());
$source_alias =& $source->value;

ReferenceStaticCopyTarget::$value = $source->value;
$registry = new ReferenceValueRegistry();
$registered = $registry->getRegistered(ReferenceStaticCopyTarget::$value['blockName']);
$has_attrs = array_key_exists('attrs', ReferenceStaticCopyTarget::$value);
$attrs_are_array = is_array(ReferenceStaticCopyTarget::$value['attrs']);

$property_target = new ReferenceSourceHolder();
$property_target->value = $source->value;
$property_target->value['blockName'] = 'property-copy';

$static_copy = ReferenceStaticCopyTarget::$value;
$static_copy['blockName'] = 'static-copy';

$local_property_copy = $source->value;
$local_property_copy['blockName'] = 'local-copy';

echo json_encode(array(
    'source' => $source->value,
    'source_alias' => $source_alias,
    'property_target' => $property_target->value,
    'static_copy_target' => ReferenceStaticCopyTarget::$value,
    'static_copy' => $static_copy,
    'local_property_copy' => $local_property_copy,
    'registered' => $registered,
    'has_attrs' => $has_attrs,
    'attrs_are_array' => $attrs_are_array,
)), "\n";
