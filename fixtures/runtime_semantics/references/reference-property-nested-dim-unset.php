<?php
// runtime-semantics: category=references expect=pass php_ref_required=1
// A local reference to an array-valued object property must preserve sibling
// keys while nested dimensions are assigned and unset through the alias.

final class NestedDimRegistry {
    private $values;

    public function __construct($values = null) {
        $this->values = $values ?? [
            'header' => ['filePath' => 'header.php'],
            'heading' => ['filePath' => 'heading.php'],
            'query' => ['filePath' => 'query.php'],
            'footer' => ['filePath' => 'footer.php'],
        ];
    }

    public function assignThroughReference($key) {
        $values = &$this->values;
        $values[$key]['content'] = 'loaded';
    }

    public function unsetThroughReference($key) {
        $values = &$this->values;
        unset($values[$key]['filePath']);
    }

    public function keys() {
        return array_keys($this->values);
    }

    public function entry($key) {
        return $this->values[$key] ?? null;
    }

    public function has($key) {
        return isset($this->values[$key]);
    }
}

$assigned = new NestedDimRegistry();
$assigned->assignThroughReference('header');
echo json_encode($assigned->keys()), "\n";
echo json_encode($assigned->entry('header')), "\n";

$unset = new NestedDimRegistry();
$unset->unsetThroughReference('header');
echo json_encode($unset->keys()), "\n";
echo json_encode($unset->entry('header')), "\n";
echo json_encode($unset->entry('query')), "\n";

$unset->unsetThroughReference('heading');
echo json_encode($unset->keys()), "\n";
echo json_encode($unset->entry('heading')), "\n";
echo json_encode($unset->entry('footer')), "\n";

$seed = [
    'header' => ['filePath' => 'header.php'],
    'heading' => ['filePath' => 'heading.php'],
    'query' => ['filePath' => 'query.php'],
    'footer' => ['filePath' => 'footer.php'],
];
$shared = new NestedDimRegistry($seed);
$shared->assignThroughReference('header');
$shared->unsetThroughReference('header');
echo json_encode($shared->keys()), "\n";
echo json_encode($shared->entry('header')), "\n";
echo json_encode($shared->entry('query')), "\n";
echo json_encode($seed), "\n";

$longKeys = [
    'twentytwentyfive/header',
    'twentytwentyfive/hidden-blog-heading',
    'twentytwentyfive/template-query-loop',
    'twentytwentyfive/footer',
];
$longSeed = [];
foreach ($longKeys as $key) {
    $longSeed[$key] = ['filePath' => $key];
}
$long = new NestedDimRegistry($longSeed);
$long->assignThroughReference($longKeys[0]);
$long->unsetThroughReference($longKeys[0]);
$long->assignThroughReference($longKeys[1]);
$long->unsetThroughReference($longKeys[1]);
foreach ($longKeys as $key) {
    echo $key, '=', $long->has($key) ? 'yes' : 'no', "\n";
}
echo json_encode($long->keys()), "\n";
