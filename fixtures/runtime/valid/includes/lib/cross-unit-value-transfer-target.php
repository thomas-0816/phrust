<?php
function cross_unit_transfer_values($array, $object, $text, $number) {
    $array['inside'] = 2;
    return array($array, $object, $text, $number + 1, 'unit-literal');
}

function cross_unit_increment(&$value) {
    $value++;
}

function cross_unit_named_increment(&$first, $second) {
    $first += $second;
}

function cross_unit_read_object_payload($carrier) {
    return $carrier->payload;
}

function cross_unit_identity($value) {
    return $value;
}

function cross_unit_read_reference_payload(&$payload) {
    return $payload;
}

function cross_unit_read_global_payload() {
    global $cross_unit_global;
    return $cross_unit_global;
}

function cross_unit_publish_registry() {
    global $cross_unit_registry;
    $cross_unit_registry = array('status' => 'registered');
}

function cross_unit_static_sequence() {
    static $storage = null;
    static $calls = 0;
    ++$calls;
    if (null === $storage) {
        $storage = new stdClass();
        $storage->first = $calls;
    }
    return array($storage, $calls);
}

class CrossUnitFilter {
    private $output;

    public function __construct($input) {
        $this->output = $input;
    }

    public function filter($args = array(), $operator = 'AND') {
        $operator = strtoupper($operator);
        if (!in_array($operator, array('AND', 'OR', 'NOT'), true)) {
            return array();
        }
        $count = count($args);
        $filtered = array();
        foreach ($this->output as $key => $obj) {
            $matched = 0;
            foreach ($args as $m_key => $m_value) {
                if (is_array($obj)) {
                    if (array_key_exists($m_key, $obj) && ($m_value == $obj[$m_key])) {
                        ++$matched;
                    }
                } elseif (is_object($obj)) {
                    if (isset($obj->{$m_key}) && ($m_value == $obj->{$m_key})) {
                        ++$matched;
                    }
                }
            }
            if (('AND' === $operator && $matched === $count)
                || ('OR' === $operator && $matched > 0)
                || ('NOT' === $operator && 0 === $matched)) {
                $filtered[$key] = $obj;
            }
        }
        $this->output = $filtered;
        return $this->output;
    }
}

class CrossUnitConstructedDefaults {
    public $defaults;

    public function __construct() {
        $this->defaults = array(
            'orderby' => 'name',
            'order' => 'ASC',
            'parent' => '',
            'cache_domain' => 'core',
        );
    }

    public function values() {
        return $this->defaults;
    }

    public function mutateDefaults() {
        return cross_unit_mutate_property($this->defaults);
    }
}

class CrossUnitNestedConstructor {
    public $value;

    public function __construct() {
        cross_unit_interlude_storage()->get('theme');
        $this->value = 'nested-constructed';
        return;
    }
}

class CrossUnitDimensionMutation {
    protected static function mutate(&$context) {
        $context['changed'] = true;
    }

    public static function run($data) {
        static::mutate($data['settings']);
        return $data;
    }
}
