<?php

#[AllowDynamicProperties]
class ExternalRichObject
{
    public $id = 1;
    public $name = 'complete';
    public $slug = 'complete-slug';
    public $count = 7;

    public function __get($name)
    {
        return 'magic-' . $name;
    }
}

function external_rich_object($id)
{
    return new ExternalRichObject();
}

class ExternalObjectQuery
{
    public function relay($terms)
    {
        return $this->populate($terms);
    }

    protected function populate($terms)
    {
        $objects = array();
        if (!is_array($terms)) {
            return $objects;
        }

        foreach ($terms as $key => $term_data) {
            if (is_object($term_data) && property_exists($term_data, 'id')) {
                $term = external_rich_object($term_data->id);
                if (property_exists($term_data, 'object_id')) {
                    $term->object_id = (int) $term_data->object_id;
                }
                if (property_exists($term_data, 'count')) {
                    $term->count = (int) $term_data->count;
                }
                $term->roundtrip_object_id = $term->object_id;
            } else {
                $term = external_rich_object($term_data);
            }

            if ($term instanceof ExternalRichObject) {
                $objects[$key] = $term;
            }
        }

        return $objects;
    }
}
