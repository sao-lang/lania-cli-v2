// 手写 Dart 端点 adaptor：只有 manifest + 两个业务回调。
// 帧编解码、stdio 循环、id、心跳、错误包装来自 protocol/gen/dart/runtime.dart。
import 'protocol.dart';
import 'runtime.dart';

void main(List<String> args) async {
  await boot(
    const EndpointManifest(
      ['echo.ping', 'echo.upper'],
      ['event.diag', 'event.agent_*'],
    ),
    const EchoHooks(),
    args,
  );
}

class EchoHooks implements EndpointHooks {
  const EchoHooks();

  @override
  RequestOutcome onRequest(String capability, dynamic params, String correlationId) {
    if (capability == 'echo.ping') {
      return RequestOutcome({
        'pong': true,
        'params': params,
        'correlationId': correlationId,
        'endpointId': 'dart-endpoint',
      }, null);
    }
    if (capability == 'echo.upper') {
      final text = params is Map ? params['text'] : null;
      if (text is! String) {
        return RequestOutcome(null, busError(eInvalidParams, 'text must be a string'));
      }
      return RequestOutcome({'text': text.toUpperCase(), 'endpointId': 'dart-endpoint'}, null);
    }
    return RequestOutcome(null, busError(eMethodNotFound, 'unsupported capability'));
  }

  @override
  void onEvent(String topic, dynamic payload, String from) {
    currentRuntime().publish('event.echo_ack', {
      'endpointId': 'dart-endpoint',
      'receivedTopic': topic,
      'from': from,
      'payload': payload,
    });
  }
}
