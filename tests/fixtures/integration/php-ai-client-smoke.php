<?php
declare(strict_types=1);

use WordPress\AiClient\Builders\MessageBuilder;
use WordPress\AiClient\Messages\Enums\ModalityEnum;
use WordPress\AiClient\Results\DTO\TokenUsage;

$source = $argv[1] ?? '';
if ($source === '' || !is_file($source . '/vendor/autoload.php')) {
    fwrite(STDERR, "php-ai-client checkout with vendor/autoload.php is required\n");
    exit(2);
}

require $source . '/vendor/autoload.php';

$usage = new TokenUsage(7, 5, 12, 2);
$message = (new MessageBuilder('native composer autoload'))
    ->usingUserRole()
    ->get();
$modality = ModalityEnum::from('text');

echo json_encode(
    array(
        'package' => 'wordpress/php-ai-client',
        'usage' => $usage->toArray(),
        'message' => $message->toArray(),
        'modality' => array(
            'name' => $modality->name,
            'value' => $modality->value,
            'is_text' => $modality->isText(),
        ),
    ),
    JSON_THROW_ON_ERROR | JSON_UNESCAPED_SLASHES
), "\n";
