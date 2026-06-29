<?php
function app_flow_scale() {
    $value = getenv('PHRUST_APP_FLOW_SCALE');
    $scale = (int)$value;
    if ($scale < 1) {
        return 1;
    }
    return $scale;
}

function app_flow_contains($haystack, $needle) {
    return strpos($haystack, $needle) !== false;
}

function app_flow_validate_request($input) {
    $errors = array();
    $payload = array();
    $score = 0;
    if (!isset($input['name']) || strlen($input['name']) === 0) {
        $errors[] = 'name.required';
        $score = $score + 13;
    } else {
        $payload['name'] = $input['name'];
        $score = $score + 7;
    }
    if (isset($input['age'])) {
        $age = (int)$input['age'];
    } else {
        $age = 0;
    }
    if ($age < 18 || $age > 120) {
        $errors[] = 'age.range';
        $score = $score + 13;
    } else {
        $payload['age'] = $age;
        $score = $score + 7;
    }
    if (isset($input['email'])) {
        $email = $input['email'];
    } else {
        $email = '';
    }
    if (!app_flow_contains($email, '@') || !app_flow_contains($email, '.')) {
        $errors[] = 'email.format';
        $score = $score + 13;
    } else {
        $payload['email'] = strtolower($email);
        $score = $score + 7;
    }
    if (isset($input['plan'])) {
        $plan = $input['plan'];
    } else {
        $plan = '';
    }
    $validPlan = false;
    if ($plan === 'free') {
        $validPlan = true;
    }
    if ($plan === 'team') {
        $validPlan = true;
    }
    if ($plan === 'pro') {
        $validPlan = true;
    }
    if (!$validPlan) {
        $errors[] = 'plan.enum';
        $score = $score + 13;
    } else {
        $score = $score + 7;
        $payload['plan'] = $plan;
    }
    if (isset($input['profile']['tags'])) {
        $tags = $input['profile']['tags'];
    } else {
        $tags = array();
    }
    $payload['tag_count'] = count($tags);
    $score = $score + count($tags);
    return array('errors' => $errors, 'payload' => $payload, 'score' => $score);
}

$requests = array(
    array('name' => ' Ada ', 'age' => '34', 'email' => 'ADA@example.test', 'plan' => 'team', 'profile' => array('tags' => array('admin', 'ops'))),
    array('age' => '15', 'email' => 'bad', 'plan' => 'other', 'profile' => array('tags' => array())),
    array('name' => 'Lin', 'age' => '44', 'email' => 'lin@example.test', 'plan' => 'pro', 'profile' => array('tags' => array('dev'))),
);
$checksum = 0;
$items = 0;
for ($round = 0; $round < app_flow_scale() * 30; $round++) {
    foreach ($requests as $request) {
        $result = app_flow_validate_request($request);
        $rowChecksum = $result['score'] + $round % 9;
        $checksum = $checksum + $rowChecksum;
        $items++;
    }
}
echo 'app-flow request_validation_errors checksum=' . $checksum . ' items=' . $items . "\n";
