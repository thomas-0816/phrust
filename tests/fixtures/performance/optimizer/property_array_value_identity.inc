<?php

class PropertyArrayValueIdentityMarker {
    public function translate($singular, $context = null) {
        return $singular;
    }
}

function property_array_value_identity_get_translator() {
    global $property_array_value_identity_translator;
    if ($property_array_value_identity_translator === null) {
        $property_array_value_identity_translator = new PropertyArrayValueIdentityMarker();
    }
    return $property_array_value_identity_translator;
}

function property_array_value_identity_filter($translation, $original) {
    return $translation;
}

function property_array_value_identity_translate(string $value) {
    $translations = property_array_value_identity_get_translator();
    $translation = $translations->translate($value);
    $translation = property_array_value_identity_filter($translation, $value);
    return $translation;
}
