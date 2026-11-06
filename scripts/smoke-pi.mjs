#!/usr/bin/env node
/**
 * `lan pi` 端到端冒烟：不需要真实模型/API key。
 *
 * 做法：起一个最小的 OpenAI 兼容 SSE 服务（/v1/chat/completions），
 * 第一轮返回一个 `read` 工具调用，第二轮把工具结果回显成文本。
 * 这样可以验证「CLI → bridge → pi → host tool exec → 会话落盘」整条链路。
 *
 * 用法：
 *   node scripts/smoke-pi.mjs [--bin <lan 可执行文件>]
 */
import { createServer } from 'node:http';
import { spawn } from 'node:child_process';
import { mkdtempSync, readdirSync, readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { join, resolve } from 'node:path';

const args = process.argv.slice(2);
const binIndex = args.indexOf('--bin');
const lanBin = resolve(
  binIndex >= 0 ? args[binIndex + 1] : 'rust/target/debug/lania-cli.exe',
);

const NOTE = 'lan pi smoke note';
// 不用 os.tmpdir()：受限环境下系统临时目录可能不可写，统一放在仓库的 target/.tmp 下。
const root = resolve('rust/target/.tmp');
mkdirSync(root, { recursive: true });
const workspace = mkdtempSync(join(root, 'pi-smoke-'));
writeFileSync(join(workspace, 'note.txt'), `${NOTE}\n`);

function sse(res, chunks) {
  res.writeHead(200, {
    'content-type': 'text/event-stream',
    'cache-control': 'no-cache',
    connection: 'keep-alive',
  });
  for (const chunk of chunks) {
    res.write(`data: ${JSON.stringify(chunk)}\n\n`);
  }
  res.write('data: [DONE]\n\n');
  res.end();
}

function textOf(content) {
  if (typeof content === 'string') {
    return content;
  }
  if (Array.isArray(content)) {
    return content
      .map((part) => (typeof part === 'string' ? part : String(part?.text ?? '')))
      .join('');
  }
  return '';
}

function chunk(delta, finishReason = null) {
  return {
    id: 'chatcmpl-smoke',
    object: 'chat.completion.chunk',
    created: Math.floor(Date.now() / 1000),
    model: 'smoke-model',
    choices: [{ index: 0, delta, finish_reason: finishReason }],
  };
}

let requestCount = 0;
const seenToolResults = [];

const server = createServer((req, res) => {
  if (!req.url?.endsWith('/chat/completions')) {
    res.writeHead(404).end('not found');
    return;
  }
  let body = '';
  req.on('data', (piece) => {
    body += piece;
  });
  req.on('end', () => {
    requestCount += 1;
    let payload = {};
    try {
      payload = JSON.parse(body);
    } catch {
      payload = {};
    }
    const messages = Array.isArray(payload.messages) ? payload.messages : [];
    const toolMessages = messages.filter((message) => message.role === 'tool');
    for (const message of toolMessages) {
      seenToolResults.push(String(message.content ?? ''));
    }
    const systemText = messages
      .filter((message) => message.role === 'system')
      .map((message) => textOf(message.content))
      .join('\n');
    const lastUserText = [...messages]
      .reverse()
      .find((message) => message.role === 'user');
    const userText = textOf(lastUserText?.content);

    // 子 agent 请求：系统提示里带标记，直接给答案（它没有 task 工具，不能递归）。
    if (systemText.includes('focused sub-agent')) {
      sse(res, [
        chunk({ role: 'assistant', content: 'subagent found: note.txt says lan pi smoke note' }),
        chunk({}, 'stop'),
      ]);
      return;
    }

    // 第二轮：已经有工具结果，直接给最终答案。
    if (toolMessages.length > 0) {
      sse(res, [
        chunk({ role: 'assistant', content: 'tool said: ' }),
        chunk({ content: String(toolMessages.at(-1)?.content ?? '').slice(0, 200) }),
        chunk({}, 'stop'),
      ]);
      return;
    }

    // 主 agent 且用户要求「委派」：调用 task 工具。
    if (/delegate/i.test(userText)) {
      sse(res, [
        chunk({
          role: 'assistant',
          content: '',
          tool_calls: [
            {
              index: 0,
              id: 'call_smoke_task',
              type: 'function',
              function: {
                name: 'task',
                arguments: JSON.stringify({
                  description: 'check note',
                  prompt: 'read note.txt and report its content',
                  read_only: true,
                }),
              },
            },
          ],
        }),
        chunk({}, 'tool_calls'),
      ]);
      return;
    }

    // 第一轮：要求读 note.txt。
    sse(res, [
      chunk({
        role: 'assistant',
        content: '',
        tool_calls: [
          {
            index: 0,
            id: 'call_smoke_1',
            type: 'function',
            function: { name: 'read', arguments: JSON.stringify({ path: 'note.txt' }) },
          },
        ],
      }),
      chunk({}, 'tool_calls'),
    ]);
  });
});

function run(args, cwd, env, input) {
  return new Promise((resolvePromise) => {
    const child = spawn(lanBin, args, {
      cwd,
      env: { ...process.env, ...env },
      stdio: [input == null ? 'ignore' : 'pipe', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (data) => {
      stdout += data;
    });
    child.stderr.on('data', (data) => {
      stderr += data;
    });
    if (input != null) {
      child.stdin.end(input);
    }
    child.on('close', (code) => resolvePromise({ code, stdout, stderr }));
  });
}

function fail(message, detail) {
  console.error(`FAIL: ${message}`);
  if (detail) {
    console.error(detail);
  }
  server.close();
  process.exit(1);
}

server.listen(0, '127.0.0.1', async () => {
  const port = server.address().port;
  const env = {
    LANIA_AGENT_BASE_URL: `http://127.0.0.1:${port}/v1`,
    LANIA_AGENT_API_KEY: 'smoke',
    LANIA_AGENT_PROVIDER: 'smoke',
    LANIA_AGENT_MODEL: 'smoke-model',
    NO_COLOR: '1',
    CLICOLOR_FORCE: '0',
  };
  mkdirSync(join(workspace, '.lan'), { recursive: true });

  const first = await run(
    ['pi', '-p', '--approval-mode', 'yolo', '--model', 'smoke-model', '--provider', 'smoke', 'read note.txt and answer'],
    workspace,
    env,
  );
  console.log(`--- run 1 (exit ${first.code}) ---`);
  console.log(first.stdout.trim() || '(no stdout)');
  if (first.stderr.trim()) {
    console.log(first.stderr.trim());
  }

  if (first.code !== 0) {
    fail('one-shot exited non-zero', first.stderr);
  }
  if (!first.stdout.includes('tool said:')) {
    fail('final assistant text missing', first.stdout);
  }
  if (requestCount < 2) {
    fail(`expected 2 model calls (tool then answer), got ${requestCount}`);
  }
  if (!seenToolResults.some((text) => text.includes('smoke note'))) {
    fail('host tool result never reached the model', JSON.stringify(seenToolResults));
  }

  const sessionDir = join(workspace, '.lan', 'agent', 'sessions');
  let files = [];
  try {
    files = readdirSync(sessionDir).filter((name) => name.endsWith('.jsonl'));
  } catch {
    files = [];
  }
  if (files.length !== 1) {
    fail(`expected exactly one session file, found ${files.length}`);
  }
  const lines = readFileSync(join(sessionDir, files[0]), 'utf8')
    .trim()
    .split('\n')
    .map((line) => JSON.parse(line));
  const kinds = lines.map((line) => line.type);
  for (const expected of ['header', 'user', 'assistant', 'tool']) {
    if (!kinds.includes(expected)) {
      fail(`session file missing ${expected} entry: ${kinds.join(',')}`);
    }
  }
  console.log(`session file: ${files[0]} (${kinds.join(' → ')})`);

  // 第二轮：--continue 必须把历史带回模型。
  const before = requestCount;
  const second = await run(
    ['pi', '-c', '-p', '--approval-mode', 'yolo', 'and now?'],
    workspace,
    env,
  );
  console.log(`--- run 2 (exit ${second.code}) ---`);
  console.log(second.stdout.trim() || '(no stdout)');
  if (second.code !== 0) {
    fail('continue run exited non-zero', second.stderr);
  }
  if (requestCount <= before) {
    fail('continue run did not call the model');
  }

  // 第三轮：--sessions 必须能看到刚才两个会话。
  const listed = await run(['pi', '--sessions'], workspace, env);
  if (listed.code !== 0) {
    fail('--sessions exited non-zero', listed.stderr);
  }
  if (!listed.stdout.includes(files[0].replace('.jsonl', ''))) {
    fail('--sessions did not list the session id', listed.stdout);
  }
  console.log(`--- run 3: --sessions ok (${files[0]}) ---`);

  // 第四轮：REPL 斜杠命令（不触发模型）。
  const repl = await run(['pi', '--no-session'], workspace, env, '/help\n/status\n/quit\n');
  if (repl.code !== 0) {
    fail('REPL exited non-zero', repl.stderr);
  }
  for (const expected of ['/compact', '/fork', 'session ', 'approval ']) {
    if (!repl.stdout.includes(expected)) {
      fail(`REPL output missing "${expected}"`, repl.stdout);
    }
  }
  console.log('--- run 4: REPL slash commands ok ---');

  // 第五轮：--json 必须在事件流之后给一条终态结果。
  const jsonRun = await run(
    ['pi', '-p', '--json', '--approval-mode', 'yolo', 'summarize note.txt'],
    workspace,
    env,
  );
  if (jsonRun.code !== 0) {
    fail('--json run exited non-zero', jsonRun.stderr);
  }
  const resultLine = jsonRun.stdout
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      try {
        return JSON.parse(line);
      } catch {
        return null;
      }
    })
    .find((parsed) => parsed && parsed.type === 'result');
  if (!resultLine) {
    fail('--json run did not emit a result line', jsonRun.stdout);
  }
  if (typeof resultLine.usage?.totalTokens !== 'number') {
    fail('result line missing usage', JSON.stringify(resultLine));
  }
  console.log(`--- run 5: --json result ok (text=${JSON.stringify(resultLine.text).slice(0, 40)}) ---`);

  // 第六轮：TUI 快照（不需要 TTY / API key）。
  const snapshot = await run(['pi', '--snapshot'], workspace, env);
  if (snapshot.code !== 0) {
    fail('--snapshot exited non-zero', snapshot.stderr);
  }
  for (const expected of ['approval required', 'always allow', 'ctx ']) {
    if (!snapshot.stdout.includes(expected)) {
      fail(`snapshot missing "${expected}"`, snapshot.stdout.slice(0, 400));
    }
  }
  console.log('--- run 6: TUI snapshot ok ---');

  // 第七轮：task 子 agent（委派 → 子 agent 独立上下文 → 结果回传）。
  const delegated = await run(
    ['pi', '-p', '--approval-mode', 'yolo', 'delegate a check of note.txt'],
    workspace,
    env,
  );
  if (delegated.code !== 0) {
    fail('task run exited non-zero', delegated.stderr);
  }
  if (!delegated.stdout.includes('subagent found')) {
    fail('subagent result did not reach the parent', delegated.stdout);
  }
  console.log('--- run 7: task subagent ok ---');

  // 第八轮：工具门禁（`custom.agent.guard`）必须在宿主内拒绝，模型拿不到结果。
  writeFileSync(
    join(workspace, 'lan.config.json'),
    JSON.stringify(
      {
        // `custom.agent` 是 lan.config 的扩展段（LanConfigSnapshot.custom）。
        custom: {
          agent: {
            guard: [
              { tool: 'read', pattern: 'note\\.txt', action: 'deny', reason: 'smoke guard' },
            ],
          },
        },
      },
      null,
      2,
    ),
  );
  const guarded = await run(
    ['pi', '-p', '--approval-mode', 'yolo', 'read note.txt'],
    workspace,
    env,
  );
  if (guarded.code !== 0) {
    fail('guard run exited non-zero', guarded.stderr);
  }
  if (!guarded.stdout.includes('blocked by project guard')) {
    fail('guard did not deny the tool call', guarded.stdout);
  }
  if (guarded.stdout.includes('smoke note')) {
    fail('guard let the tool execute', guarded.stdout);
  }
  console.log('--- run 8: tool guard denied the call ---');

  server.close();
  console.log('\nPASS: lan pi 端到端链路正常');
});
