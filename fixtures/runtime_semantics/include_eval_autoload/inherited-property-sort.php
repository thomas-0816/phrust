<?php
// runtime-semantics: category=include_eval_autoload expect=pass php_ref_required=1

function inherited_property_sort_callback($value)
{
    echo $value, "\n";
}

class InheritedPropertySortBase
{
    protected $groups = array(
        'event' => array(
            0 => array('inherited_property_sort_callback'),
        ),
    );

    public function dispatch($name, $parameters)
    {
        ksort($this->groups[$name]);
        foreach ($this->groups[$name] as $callbacks) {
            foreach ($callbacks as $callback) {
                $callback(...$parameters);
            }
        }
    }
}

class InheritedPropertySortChild extends InheritedPropertySortBase
{
}

(new InheritedPropertySortChild())->dispatch('event', array('child'));
