<?php
// runtime-semantics: category=wp_autoload_stdlib expect=pass
const PACK_B_CONST = "const";
define("PACK_B_DEFINED", "defined");
function pack_b_user_function() {
    return "function";
}
interface PackBFeatureInterface {}
trait PackBFeatureTrait {}
class PackBFeatureClass {
    public $prop = 1;
    public function method() {
        return "method";
    }
}
enum PackBFeatureEnum {
    case One;
}

var_dump(function_exists("pack_b_user_function"));
var_dump(function_exists("strlen"));
var_dump(class_exists("PackBFeatureClass", false));
var_dump(interface_exists("PackBFeatureInterface", false));
var_dump(trait_exists("PackBFeatureTrait", false));
var_dump(enum_exists("PackBFeatureEnum", false));
var_dump(method_exists("PackBFeatureClass", "method"));
var_dump(property_exists("PackBFeatureClass", "prop"));
var_dump(defined("PACK_B_CONST"));
var_dump(defined("PACK_B_DEFINED"));
echo constant("PACK_B_CONST"), "|", constant("PACK_B_DEFINED"), "\n";
$functions = get_defined_functions();
echo in_array("pack_b_user_function", $functions["user"], true) ? "fn=yes\n" : "fn=no\n";
echo in_array("strlen", $functions["internal"], true) ? "internal=yes\n" : "internal=no\n";
echo in_array("PackBFeatureClass", get_declared_classes(), true) ? "class=yes\n" : "class=no\n";
echo in_array("PackBFeatureInterface", get_declared_interfaces(), true) ? "interface=yes\n" : "interface=no\n";
echo in_array("PackBFeatureTrait", get_declared_traits(), true) ? "trait=yes\n" : "trait=no\n";
$constants = get_defined_constants();
echo array_key_exists("PACK_B_CONST", $constants) ? "const=yes\n" : "const=no\n";
echo array_key_exists("PACK_B_DEFINED", $constants) ? "define=yes\n" : "define=no\n";
echo extension_loaded("standard") ? "standard=yes\n" : "standard=no\n";
echo in_array("standard", get_loaded_extensions(), true) ? "loaded=yes\n" : "loaded=no\n";
