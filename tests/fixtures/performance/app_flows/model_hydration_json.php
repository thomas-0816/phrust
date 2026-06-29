<?php
function app_flow_scale() {
    $value = getenv('PHRUST_APP_FLOW_SCALE');
    $scale = (int)$value;
    if ($scale < 1) {
        return 1;
    }
    return $scale;
}

class AppFlowUserDto {
    private $id;
    private $name;
    private $active;

    public function __construct($id, $name, $active) {
        $this->id = (int)$id;
        $this->name = (string)$name;
        $this->active = (bool)$active;
    }

    public function toArray() {
        return array(
            'id' => $this->id,
            'label' => $this->id . ':' . strtoupper($this->name),
            'active' => $this->active,
        );
    }
}

$rows = array(
    array('id' => '1', 'name' => 'ada', 'active' => '1'),
    array('id' => '2', 'name' => 'lin', 'active' => ''),
    array('id' => '3', 'name' => 'max', 'active' => '1'),
);
$checksum = 0;
$items = 0;
for ($round = 0; $round < app_flow_scale() * 25; $round++) {
    $response = array();
    foreach ($rows as $row) {
        $dto = new AppFlowUserDto($row['id'], $row['name'], $row['active']);
        $response[] = $dto->toArray();
        $items++;
    }
    $encoded = json_encode($response);
    $checksum += strlen($encoded) + $round;
}
echo 'app-flow model_hydration_json checksum=' . $checksum . ' items=' . $items . "\n";
