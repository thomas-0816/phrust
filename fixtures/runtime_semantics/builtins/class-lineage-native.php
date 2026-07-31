<?php
// runtime-semantics: category=builtins expect=pass

interface NativeRootContract {}
interface NativeChildContract extends NativeRootContract {}

class NativeLineageBase
{
    public string $declared = 'native';

    public function inheritedMethod(): void {}
}
class NativeLineageChild extends NativeLineageBase implements NativeChildContract {}

$object = new NativeLineageChild();
var_dump(is_a($object, NativeLineageBase::class));
var_dump(is_a(NativeLineageChild::class, NativeLineageBase::class));
var_dump(is_a(NativeLineageChild::class, NativeLineageBase::class, true));

$interfaces = class_implements($object, false);
var_dump(isset($interfaces[NativeChildContract::class]));
var_dump(isset($interfaces[NativeRootContract::class]));
var_dump(array_keys($interfaces));
var_dump(method_exists($object, 'inheritedMethod'));
var_dump(property_exists($object, 'declared'));
