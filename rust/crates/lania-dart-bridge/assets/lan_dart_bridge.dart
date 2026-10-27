#!/usr/bin/env dart
// lan-dart-bridge —— Dart 运行时 bridge（jsonl stdio 协议）。
//
// 设计约束（docs/design/multi-runtime-bridge.zh-CN.md §3.1，与 node-bridge /
// lan-py-bridge / lan-go-bridge 完全一致）：
//
//  1. stdout 只输出协议 envelope，所有日志走 stderr。
//  2. 事件必须先于对应 request 的 response 发出。
//  3. 逐行 JSON，禁止跨行、禁止 BOM。
//  4. 读 stdin EOF 即退出（父进程消失时自杀，避免孤儿进程）。
//  5. 长任务保持 stdout 打开并持续 emit。
//  6. 被劫持的工具输出走 `event.diag`（带 seq / channel）。
//  7. 产出统一 `Diagnostic`（字段与 Rust `Diagnostic` 完全一致）。
//
// 为什么是「脚本 + `dart run`」而不是 AOT 二进制：
// `dart compile exe` 无法交叉编译（§9.1 约束 2），为它增加 3 个平台包的成本
// 与收益不成比例。脚本形态与 `lan-py-bridge` 完全同构：随二进制内嵌、
// 落到 `LANIA_BRIDGE_HOME`、`dart <script>` 直接解释执行 —— 零平台包。
//
// 子进程一律用 `Platform.resolvedExecutable`（当前 Dart VM 的绝对路径）：
// 这样既避开了 Windows 上 `dart.bat` 不能被 CreateProcess 直接拉起的问题，
// 也保证 bridge 与 `dart analyze` / `dart compile` 用的是同一个 SDK。

import 'dart:async';
import 'dart:convert';
import 'dart:io';

const String _bridgeName = 'lan-dart-bridge';
const String _protocolVersion = '0.1.0';
const String _version = '0.1.0';
const int _heartbeatIntervalMs = 10000;
const int _maxPendingEvents = 32;
const String _failureStrategy = 'reconnect';

const List<String> _methods = <String>[
  'bridge.ping',
  'bridge.heartbeat',
  'bridge.shutdown',
  'lint.dart-analyze',
  'build.dart',
];

const List<String> _events = <String>[
  'event.ready',
  'event.log',
  'event.diag',
  'event.lint_start',
  'event.lint_result',
  'event.lint_summary',
  'event.shutdown',
  'event.heartbeat',
];

int _seq = 0;

// ---------------------------------------------------------------------------
// 协议输出
// ---------------------------------------------------------------------------

void _log(String message) => stderr.writeln(message);

int _nextSeq() => ++_seq;

/// stdout 写入队列。
///
/// Dart 的 `IOSink` 在一次 `flush()` 未完成时会拒绝新的写入
/// （`Bad state: StreamSink is bound to a stream`）—— 连续 emit
/// （`lint_start` → `event.diag` → `lint_result` → `response`）必然撞上这个约束。
/// 因此写 + flush 串成 FIFO 队列：顺序不变，且每次写完都真正落盘（§10 行缓冲风险）。
Future<void> _outbox = Future<void>.value();

void _emit(Map<String, Object?> envelope) {
  final String line = jsonEncode(envelope);
  _outbox = _outbox.then((_) async {
    try {
      stdout.writeln(line);
      await stdout.flush();
    } catch (error) {
      _log('failed to write envelope: $error');
    }
  });
}

void _emitEvent(String method, Map<String, Object?> params, String requestId) {
  _emit(<String, Object?>{
    'type': 'event',
    'requestId': requestId.isEmpty ? 'bridge' : requestId,
    'payload': <String, Object?>{'method': method, 'params': params},
  });
}

void _emitDiag(
  String requestId,
  String text, {
  String stream = 'stdout',
  String channel = 'lint',
  String level = 'info',
  String? tool,
}) {
  final Map<String, Object?> params = <String, Object?>{
    'source': tool == null ? 'runtime' : 'tool',
    'stream': stream,
    'channel': channel,
    'level': level,
    'text': text,
    'seq': _nextSeq(),
    'ts': DateTime.now().millisecondsSinceEpoch,
  };
  if (tool != null) {
    params['tool'] = tool;
  }
  _emitEvent('event.diag', params, requestId);
}

void _emitResponse(
    String requestId, Object? result, Map<String, Object?>? error) {
  _emit(<String, Object?>{
    'type': 'response',
    'payload': <String, Object?>{
      'id': requestId,
      'result': result,
      'error': error
    },
  });
}

Map<String, Object?> _errorPayload(String code, String message) =>
    <String, Object?>{'code': code, 'message': message, 'data': null};

// ---------------------------------------------------------------------------
// 诊断归一化（与 Rust `normalize_diagnostics` / TS `diagnostics.ts` 一致）
// ---------------------------------------------------------------------------

const Map<String, int> _severityRank = <String, int>{
  'error': 0,
  'warning': 1,
  'info': 2,
  'hint': 3,
};

Map<String, Object?> _diagnostic(
  String tool, {
  required String severity,
  required String message,
  String? id,
  String? file,
  int? line,
  int? col,
  bool fixable = false,
}) {
  final Map<String, Object?> diagnostic = <String, Object?>{
    'tool': tool,
    'severity': severity,
    'message': message,
  };
  if (id != null && id.isNotEmpty) {
    diagnostic['id'] = id;
  }
  if (file != null && file.isNotEmpty) {
    diagnostic['file'] = file;
  }
  if (line != null) {
    diagnostic['range'] = <String, Object?>{
      'start': <String, Object?>{'line': line, 'col': col ?? 1},
    };
  }
  if (fixable) {
    diagnostic['fix'] = <String, Object?>{'available': true, 'kind': 'safe'};
  }
  return diagnostic;
}

int _startLine(Map<String, Object?> item) {
  final Object? range = item['range'];
  if (range is Map) {
    final Object? start = range['start'];
    if (start is Map) {
      final Object? line = start['line'];
      if (line is int) {
        return line;
      }
    }
  }
  return 0;
}

int _startCol(Map<String, Object?> item) {
  final Object? range = item['range'];
  if (range is Map) {
    final Object? start = range['start'];
    if (start is Map) {
      final Object? col = start['col'];
      if (col is int) {
        return col;
      }
    }
  }
  return 0;
}

List<Map<String, Object?>> _normalize(List<Map<String, Object?>> items) {
  final Set<String> seen = <String>{};
  final List<Map<String, Object?>> deduped = <Map<String, Object?>>[];
  for (final Map<String, Object?> item in items) {
    final String key = <Object?>[
      item['tool'],
      item['file'],
      _startLine(item),
      _startCol(item),
      item['id'],
    ].join('\u0000');
    if (seen.add(key)) {
      deduped.add(item);
    }
  }
  deduped.sort((Map<String, Object?> left, Map<String, Object?> right) {
    final int byFile = ((left['file'] as String?) ?? '')
        .compareTo((right['file'] as String?) ?? '');
    if (byFile != 0) return byFile;
    final int byLine = _startLine(left).compareTo(_startLine(right));
    if (byLine != 0) return byLine;
    final int byCol = _startCol(left).compareTo(_startCol(right));
    if (byCol != 0) return byCol;
    final int bySeverity = (_severityRank[left['severity']] ?? 3)
        .compareTo(_severityRank[right['severity']] ?? 3);
    if (bySeverity != 0) return bySeverity;
    return ((left['id'] as String?) ?? '')
        .compareTo((right['id'] as String?) ?? '');
  });
  return deduped;
}

Map<String, Object?> _lintResult(
  String adaptor,
  String implementation,
  List<Map<String, Object?>> diagnostics, {
  String? message,
}) {
  final List<Map<String, Object?>> normalized = _normalize(diagnostics);
  // `files` 由 `diagnostics` 派生，保证两个字段永不失配。
  final Map<String, Map<String, Object?>> byFile =
      <String, Map<String, Object?>>{};
  for (final Map<String, Object?> item in normalized) {
    final String filePath = (item['file'] as String?) ?? '.';
    final Map<String, Object?> entry = byFile.putIfAbsent(
      filePath,
      () => <String, Object?>{'filePath': filePath, 'errors': 0, 'warnings': 0},
    );
    if (item['severity'] == 'error') {
      entry['errors'] = (entry['errors'] as int) + 1;
    } else {
      entry['warnings'] = (entry['warnings'] as int) + 1;
    }
  }
  int errors = 0;
  int warnings = 0;
  for (final Map<String, Object?> entry in byFile.values) {
    errors += entry['errors'] as int;
    warnings += entry['warnings'] as int;
  }
  final Map<String, Object?> result = <String, Object?>{
    'adaptor': adaptor,
    'implementation': implementation,
    'errors': errors,
    'warnings': warnings,
    'files': byFile.values.toList(),
    'diagnostics': normalized,
    'tool': adaptor,
  };
  if (message != null && message.isNotEmpty) {
    result['message'] = message;
  }
  return result;
}

// ---------------------------------------------------------------------------
// 解析器（与 ts/packages/node-bridge/src/plugins/lint/parsers.ts 逐字段对齐）
// ---------------------------------------------------------------------------

/// `dart analyze`：人类可读形态 `SEVERITY|TYPE|CODE|file|line|col|len|message`。
///
/// 注意：`--format machine` 的输出里 message 自身可能含 `|`，
/// 因此只按前 7 个分隔符切分，剩余部分整体作为消息。
List<Map<String, Object?>> parseDartAnalyze(String output) {
  final List<Map<String, Object?>> diagnostics = <Map<String, Object?>>[];
  for (final String line in const LineSplitter().convert(output)) {
    final List<String> fields = line.split('|');
    if (fields.length < 8) {
      continue;
    }
    final String severityRaw = fields[0];
    final String code = fields[2];
    final String file = fields[3];
    if (file.isEmpty) {
      continue;
    }
    final int? lineNumber = int.tryParse(fields[4]);
    final int? col = int.tryParse(fields[5]);
    final String severity = severityRaw == 'ERROR'
        ? 'error'
        : severityRaw == 'WARNING'
            ? 'warning'
            : 'info';
    diagnostics.add(_diagnostic(
      'dart-analyze',
      severity: severity,
      message: fields.sublist(7).join('|').trim(),
      id: code,
      file: file,
      line: lineNumber,
      col: col,
    ));
  }
  return diagnostics;
}

/// `dart compile` 的错误：`file:line:col: message`（与 go/rust 形态一致）。
final RegExp _locationLine = RegExp(r'^(.*):(\d+):(\d+):\s*(.*)$');

List<Map<String, Object?>> parseLocationText(String tool, String output) {
  final List<Map<String, Object?>> diagnostics = <Map<String, Object?>>[];
  for (final String raw in const LineSplitter().convert(output)) {
    final RegExpMatch? match = _locationLine.firstMatch(raw.trim());
    if (match == null) {
      continue;
    }
    final int? lineNumber = int.tryParse(match.group(2)!);
    final int? col = int.tryParse(match.group(3)!);
    final String message = match.group(4)!.trim();
    if (lineNumber == null || col == null || message.isEmpty) {
      continue;
    }
    diagnostics.add(_diagnostic(
      tool,
      severity: 'error',
      message: message,
      file: match.group(1),
      line: lineNumber,
      col: col,
    ));
  }
  return diagnostics;
}

// ---------------------------------------------------------------------------
// 工具执行
// ---------------------------------------------------------------------------

class _Outcome {
  _Outcome(this.exitCode, this.stdoutText, this.stderrText, [this.failure]);

  final int exitCode;
  final String stdoutText;
  final String stderrText;
  final String? failure;
}

/// 用当前 Dart VM 执行一次工具（`runInShell: false`：参数原样传递，不经 shell）。
Future<_Outcome> _run(List<String> args, String cwd) async {
  try {
    final ProcessResult result = await Process.run(
      Platform.resolvedExecutable,
      args,
      workingDirectory: cwd,
      environment: <String, String>{'FORCE_COLOR': '0'},
      runInShell: false,
      stdoutEncoding: utf8,
      stderrEncoding: utf8,
    );
    return _Outcome(
      result.exitCode,
      result.stdout as String? ?? '',
      result.stderr as String? ?? '',
    );
  } on ProcessException catch (error) {
    return _Outcome(127, '', error.message, error.message);
  }
}

/// 流式执行：逐行转成 `event.diag`，同时保留整段输出供解析器使用。
Future<_Outcome> _runStreaming(
  List<String> args,
  String cwd,
  String requestId,
  String tool,
  String channel,
) async {
  late final Process process;
  try {
    process = await Process.start(
      Platform.resolvedExecutable,
      args,
      workingDirectory: cwd,
      environment: <String, String>{'FORCE_COLOR': '0'},
      runInShell: false,
    );
  } on ProcessException catch (error) {
    return _Outcome(127, '', error.message, error.message);
  }

  final StringBuffer stdoutBuffer = StringBuffer();
  final StringBuffer stderrBuffer = StringBuffer();
  final List<Future<void>> pumps = <Future<void>>[
    process.stdout
        .transform(utf8.decoder)
        .transform(const LineSplitter())
        .forEach((String line) {
      stdoutBuffer.writeln(line);
      if (line.trim().isNotEmpty) {
        _emitDiag(requestId, line,
            stream: 'stdout', channel: channel, tool: tool);
      }
    }),
    process.stderr
        .transform(utf8.decoder)
        .transform(const LineSplitter())
        .forEach((String line) {
      stderrBuffer.writeln(line);
      if (line.trim().isNotEmpty) {
        _emitDiag(
          requestId,
          line,
          stream: 'stderr',
          channel: channel,
          level: line.toLowerCase().contains('error') ? 'error' : 'info',
          tool: tool,
        );
      }
    }),
  ];
  final int exitCode = await process.exitCode;
  await Future.wait(pumps);
  return _Outcome(exitCode, stdoutBuffer.toString(), stderrBuffer.toString());
}

List<String> _stringList(Object? value) {
  if (value is List) {
    return value.whereType<String>().toList();
  }
  return <String>[];
}

String _stringParam(Map<String, Object?> params, String key, String fallback) {
  final Object? value = params[key];
  if (value is String && value.isNotEmpty) {
    return value;
  }
  return fallback;
}

/// `dart` / `dart.exe` / 任意路径形式的 dart 可执行文件。
bool _isDartProgram(String argument) {
  final String name = argument.split(RegExp(r'[\\/]')).last.toLowerCase();
  return name == 'dart' || name == 'dart.exe';
}

// ---------------------------------------------------------------------------
// 请求处理
// ---------------------------------------------------------------------------

Future<Map<String, Object?>> _handleLint(
  String adaptor,
  Map<String, Object?> params,
  String requestId,
) async {
  final String cwd = _stringParam(params, 'cwd', Directory.current.path);
  List<String> args = _stringList(params['args']);

  _emitDiag(requestId, 'running $adaptor', channel: 'lint', tool: adaptor);

  if (adaptor != 'dart-analyze' && adaptor != 'analyze') {
    return _lintResult(
      adaptor,
      'fallback',
      <Map<String, Object?>>[],
      message: 'unsupported lint adaptor: $adaptor',
    );
  }
  adaptor = 'dart-analyze';
  if (args.isEmpty) {
    args = <String>['analyze', '--format', 'machine'];
  }
  final _Outcome outcome = await _run(args, cwd);
  if (outcome.failure != null) {
    // 连 `Platform.resolvedExecutable` 都拉不起来：是真的失败，必须可见。
    return _failedLintResult(
        adaptor, 'dart sdk unavailable: ${outcome.failure}');
  }
  final List<Map<String, Object?>> diagnostics =
      parseDartAnalyze('${outcome.stdoutText}\n${outcome.stderrText}');
  // `dart analyze` 发现问题是 3（< 64 的非零）；其余非零退出码意味着工具没跑起来。
  if (diagnostics.isEmpty && outcome.exitCode != 0) {
    return _failedLintResult(
      adaptor,
      outcome.stderrText.trim().isEmpty
          ? outcome.stdoutText
          : outcome.stderrText,
    );
  }
  return _lintResult(adaptor, 'runtime', diagnostics);
}

/// 工具**执行失败**（不是「没问题」）必须是一条可见诊断。
///
/// 否则 `lan lint` 会因为 errors=0 而以 0 退出、报告里什么都不显示 ——
/// 用户既看不到失败原因，也拿不到非零退出码。与 TS `failedRuntimeResult` 同语义。
Map<String, Object?> _failedLintResult(String adaptor, String message) {
  final String trimmed = message.trim();
  return _lintResult(adaptor, 'runtime', <Map<String, Object?>>[
    _diagnostic(
      adaptor,
      severity: 'error',
      message: trimmed.isEmpty ? '$adaptor failed' : trimmed,
    ),
  ]);
}

Future<Map<String, Object?>> _handleBuild(
  Map<String, Object?> params,
  String requestId,
) async {
  final String cwd = _stringParam(params, 'cwd', Directory.current.path);
  final String output = _stringParam(params, 'outputDir', 'bin/app');
  List<String> command = _stringList(params['command']);
  if (command.isEmpty) {
    command = <String>['compile', 'exe', 'bin/main.dart'];
  } else if (_isDartProgram(command.first)) {
    // toolchain 表里 Go/Dart 的命令都带着可执行文件本身（`['dart','compile',…]`）。
    // 这里固定用 `Platform.resolvedExecutable`（同一个 SDK），所以要丢掉首元素。
    command = command.sublist(1);
  }
  // `{output}` 占位符由 toolchain 表定义（Go 用它）；Dart 的默认命令没有该占位，
  // 因此这里补一个 `-o <output>`，让 `--output-dir` 对 Dart 项目同样生效。
  bool hasOutput = false;
  command = command.map((String arg) {
    if (arg.contains('{output}')) {
      hasOutput = true;
      return arg.replaceAll('{output}', output);
    }
    return arg;
  }).toList();
  if (!hasOutput && command.isNotEmpty && command.first == 'compile') {
    command = <String>[...command, '-o', output];
  }

  final _Outcome outcome =
      await _runStreaming(command, cwd, requestId, 'dart', 'build');
  final List<Map<String, Object?>> diagnostics =
      _normalize(<Map<String, Object?>>[
    ...parseLocationText('dart', outcome.stderrText),
    ...parseLocationText('dart', outcome.stdoutText),
  ]);
  final bool success = outcome.failure == null && outcome.exitCode == 0;
  final Map<String, Object?> result = <String, Object?>{
    'success': success,
    'tool': 'dart',
    'runtime': 'dart',
    'action': 'build',
    'bridge': _bridgeName,
    'exitCode': outcome.exitCode,
    'outputDir': output,
    'diagnostics': diagnostics,
    'command': command,
  };
  if (!success) {
    final String message = outcome.stderrText.trim().isEmpty
        ? (outcome.failure ?? 'dart build exited with code ${outcome.exitCode}')
        : outcome.stderrText.trim().split('\n').last.trim();
    result['error'] = message;
  }
  return result;
}

Future<bool> _handleRequest(Map<String, Object?> envelope) async {
  final Object? rawPayload = envelope['payload'];
  final Map<String, Object?> payload = rawPayload is Map
      ? rawPayload.cast<String, Object?>()
      : <String, Object?>{};
  final String requestId = (payload['id'] as String?) ?? '';
  final String method = (payload['method'] as String?) ?? '';
  final Object? rawParams = payload['params'];
  final Map<String, Object?> params = rawParams is Map
      ? rawParams.cast<String, Object?>()
      : <String, Object?>{};

  switch (method) {
    case 'bridge.ping':
      _emitResponse(
          requestId,
          <String, Object?>{
            'ok': true,
            'bridgeName': _bridgeName,
            'version': _version,
          },
          null);
      return true;
    case 'bridge.heartbeat':
      _emitResponse(
          requestId,
          <String, Object?>{
            'ok': true,
            'ts': DateTime.now().millisecondsSinceEpoch,
          },
          null);
      return true;
    case 'bridge.shutdown':
      _emitEvent(
          'event.shutdown', <String, Object?>{'reason': 'shutdown'}, requestId);
      _emitResponse(requestId, <String, Object?>{'ok': true}, null);
      return false;
  }

  if (method.startsWith('lint.')) {
    final String adaptor = method.substring('lint.'.length);
    _emitEvent(
        'event.lint_start',
        <String, Object?>{
          'adaptors': <String>[adaptor],
        },
        requestId);
    try {
      final Map<String, Object?> result =
          await _handleLint(adaptor, params, requestId);
      _emitEvent(
          'event.lint_result',
          <String, Object?>{
            'adaptor': adaptor,
            'errors': result['errors'],
            'warnings': result['warnings'],
            'implementation': result['implementation'],
          },
          requestId);
      _emitResponse(requestId, result, null);
    } catch (error) {
      // bridge 不能因为单个工具崩掉；原因同时写到 stderr，便于定位。
      _log('lint.$adaptor failed: $error');
      _emitResponse(
          requestId, null, _errorPayload('E_LINT_FAILED', error.toString()));
    }
    return true;
  }

  if (method == 'build.dart') {
    try {
      final Map<String, Object?> result = await _handleBuild(params, requestId);
      _emitResponse(requestId, result, null);
    } catch (error) {
      _log('build.dart failed: $error');
      _emitResponse(
          requestId, null, _errorPayload('E_BUILD_FAILED', error.toString()));
    }
    return true;
  }

  _emitResponse(
    requestId,
    null,
    _errorPayload('E_UNSUPPORTED', 'unsupported method: $method'),
  );
  return true;
}

Map<String, Object?> _handshakeResponse(Map<String, Object?> request) {
  final Object? protocol = request['protocolVersion'];
  return <String, Object?>{
    'protocolVersion':
        protocol is String && protocol.isNotEmpty ? protocol : _protocolVersion,
    'bridgeName': _bridgeName,
    'methods': _methods,
    'events': _events,
    'heartbeatIntervalMs': _heartbeatIntervalMs,
    'maxPendingEvents': _maxPendingEvents,
    'failureStrategy': _failureStrategy,
  };
}

Future<int> _serve() async {
  _emitEvent(
      'event.ready',
      <String, Object?>{
        'bridgeName': _bridgeName,
        'methods': _methods,
        'pid': pid,
      },
      '');

  // EOF（父进程消失）即退出，避免孤儿进程（§3.1 约定 4）。
  await for (final String line
      in stdin.transform(utf8.decoder).transform(const LineSplitter())) {
    final String trimmed = line.trim();
    if (trimmed.isEmpty) {
      continue;
    }
    Map<String, Object?> envelope;
    try {
      final Object? decoded = jsonDecode(trimmed);
      if (decoded is! Map) {
        continue;
      }
      envelope = decoded.cast<String, Object?>();
    } on FormatException catch (error) {
      _log('failed to parse envelope: $error');
      continue;
    }
    final Object? type = envelope['type'];
    if (type == 'handshake') {
      final Object? rawRequest = envelope['payload'];
      _emit(<String, Object?>{
        'type': 'handshake',
        'payload': _handshakeResponse(rawRequest is Map
            ? rawRequest.cast<String, Object?>()
            : <String, Object?>{}),
      });
      continue;
    }
    if (type == 'request') {
      final bool keepGoing = await _handleRequest(envelope);
      if (!keepGoing) {
        return 0;
      }
      continue;
    }
    if (type == 'host_response') {
      // 本 bridge 不发起 host RPC，忽略。
      continue;
    }
    _log('unsupported envelope type: $type');
  }
  return 0;
}

Future<void> main(List<String> argv) async {
  if (argv.contains('--version')) {
    stderr.writeln('$_bridgeName $_version');
    return;
  }
  final int code = await _serve();
  // 退出前必须把队列排空，否则最后一条 response 可能还在队列里。
  await _outbox;
  exit(code);
}
