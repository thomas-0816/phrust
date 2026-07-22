<?php
// runtime-semantics: category=refs expect=pass php_ref_required=1

function exercise_local_array_lvalues(): void
{
    $source = 3;
    $array = ['outer' => ['leaf' => 1, 'plain' => 2]];
    $copy = $array;

    $array['outer']['leaf'] =& $source;
    $source = 7;
    echo $copy['outer']['leaf'], '|', $array['outer']['leaf'], '|';

    $alias =& $array['outer']['plain'];
    $alias = 9;
    echo $array['outer']['plain'], '|';

    $missing =& $array['created'];
    $missing = 11;
    echo $array['created'], '|';

    $text = 'source-string';
    $array['text'] =& $text;
    $text = 'changed-string';
    $text_alias =& $array['text'];
    echo $text_alias, '|';

    $appended = 4;
    $array['outer'][] =& $appended;
    $appended = 5;
    echo $array['outer'][0], "\n";
}

class NativeStaticLvalueBox
{
    public static $value = 'old-static';
    public static $items = [
        'leaf' => 'old-leaf',
        'remove' => 'present',
        'nested' => ['empty' => ''],
    ];

    public static function bind(): void
    {
        $source = 'source-static';
        self::$value =& $source;
        $source = 'changed-static';
        echo self::$value, "\n";
    }

    public static function bindLeaf(): void
    {
        $leaf =& self::$items['leaf'];
        $leaf = 'changed-leaf';
        echo self::$items['leaf'], '|';
        echo isset(self::$items['nested']['empty']) ? 'set' : 'missing', '|';
        echo empty(self::$items['nested']['empty']) ? 'empty' : 'filled', '|';
        unset(self::$items['remove']);
        echo isset(self::$items['remove']) ? 'present' : 'removed', "\n";
    }
}

function &native_returned_leaf(array &$array)
{
    $leaf =& $array['leaf'];
    return $leaf;
}

function exercise_native_reference_return(): void
{
    $array = ['leaf' => 'before-return'];
    $returned =& native_returned_leaf($array);
    $returned = 'after-return';
    echo $array['leaf'], "\n";
}

function native_foreach_reference_plan(): void
{
    $values = ['first' => 1, 'second' => 2];
    $copy = $values;
    foreach ($values as $key => &$value) {
        $value += 10;
    }
    $value = 99;
    echo $copy['first'], '|', $copy['second'], '|';
    echo $values['first'], '|', $values['second'], "\n";
}

function native_static_local_plan(): int
{
    static $counter = 0;
    return ++$counter;
}

function native_float_echo_plan(): void
{
    echo 1.5, '|', -0.0, '|', 1.0e-5, '|', 1.0e14, "\n";
    $rendered = (string) -0.0;
    echo $rendered, '|', (string) 1.0e14, "\n";
    $integer = (int) 1.5;
    echo $integer, '|', ~1.0, "\n";
}

class NativeObjectDimBox
{
    public array $items = ['leaf' => '', 'nested' => ['value' => 1]];
    public $scalar = 'value';

    public function probe(): void
    {
        echo $this::class, '|';
        echo isset($this->items) ? 'property-set' : 'property-missing', '|';
        echo empty($this->items) ? 'property-empty' : 'property-filled', '|';
        echo isset($this->items['nested']['value']) ? 'dim-set' : 'dim-missing', '|';
        echo empty($this->items['leaf']) ? 'dim-empty' : 'dim-filled', "\n";
    }

    public function mutate(): void
    {
        $this->items['nested']['value'] = 2;
        $this->items['added'] = 'added';
        $this->items[] = 'tail';
        unset($this->items['leaf']);
        echo $this->items['nested']['value'], '|', $this->items['added'], '|';
        echo $this->items[0], '|';
        echo isset($this->items['leaf']) ? 'leaf-present' : 'leaf-removed', "\n";
    }

    public function unsetScalar(): void
    {
        unset($this->scalar);
        echo isset($this->scalar) ? 'scalar-present' : 'scalar-removed', "\n";
    }
}

function native_new_object_plan(): NativeObjectDimBox
{
    $object = new NativeObjectDimBox();
    $object->items['nested']['value'] = 7;
    return clone $object;
}

exercise_local_array_lvalues();
NativeStaticLvalueBox::bind();
NativeStaticLvalueBox::bindLeaf();
exercise_native_reference_return();
$foreach_values = ['first' => 1, 'second' => 2];
$foreach_copy = $foreach_values;
foreach ($foreach_values as $foreach_key => &$foreach_value) {
    $foreach_value += 10;
}
$foreach_value = 99;
echo $foreach_copy['first'], '|', $foreach_copy['second'], '|';
echo $foreach_values['first'], '|', $foreach_values['second'], "\n";
echo native_static_local_plan(), '|', native_static_local_plan(), '|';
echo native_static_local_plan(), "\n";
native_float_echo_plan();
$object_box = native_new_object_plan();
$object_box->probe();
$object_box->probe();
$object_box->mutate();
$object_box->mutate();
$object_box->unsetScalar();
$object_box->unsetScalar();
