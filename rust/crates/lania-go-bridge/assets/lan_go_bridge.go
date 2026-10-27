// lan-go-bridge —— Go 运行时 bridge（jsonl stdio 协议）。
//
// 设计约束（docs/design/multi-runtime-bridge.zh-CN.md §3.1，与 node-bridge /
// lan-py-bridge 完全一致）：
//
//  1. stdout 只输出协议 envelope，所有日志走 stderr。
//  2. 事件必须先于对应 request 的 response 发出。
//  3. 逐行 JSON，禁止跨行、禁止 BOM。
//  4. 读 stdin EOF 即退出（父进程消失时自杀，避免孤儿进程）。
//  5. 长任务保持 stdout 打开并持续 emit。
//  6. 被劫持的工具输出走 `event.diag`（带 seq / channel）。
//  7. 产出统一 `Diagnostic`（字段与 Rust `Diagnostic` 完全一致）。
//
// 只用标准库实现：这样 `go build` 不需要任何模块下载 / 网络，
// 交叉编译（§9.1 约束 1「不得依赖 CGO」）也天然成立。
//
// 为什么 `go vet` 由本进程 spawn 而不是进程内 `go/analysis`：
// `go/analysis` 的 analyzer 实现（printf / lostcancel …）都在
// `golang.org/x/tools`，把它引入进来会让 bridge 的构建依赖模块下载，
// 与「零第三方依赖、可离线构建」冲突。因此 B10 的收益定为：
// **去掉 Node 中转（Rust → go bridge → go vet/go build，2 层）** +
// 统一 `Diagnostic` / `event.diag`，而不是把 analyzer 搬进进程内。
package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"
)

const (
	bridgeName       = "lan-go-bridge"
	protocolVersion  = "0.1.0"
	heartbeatMs      = 10000
	maxPendingEvents = 32
	failureStrategy  = "reconnect"
	version          = "0.1.0"
)

var bridgeMethods = []string{
	"bridge.ping",
	"bridge.heartbeat",
	"bridge.shutdown",
	"lint.go-vet",
	"lint.golangci-lint",
	"build.go",
}

// 只声明本 bridge 会发出的事件（与 Rust `BridgeEventMethod` 的交集）。
var bridgeEvents = []string{
	"event.ready",
	"event.log",
	"event.diag",
	"event.lint_start",
	"event.lint_result",
	"event.lint_summary",
	"event.shutdown",
	"event.heartbeat",
}

// ---------------------------------------------------------------------------
// 诊断模型（§7.3）
// ---------------------------------------------------------------------------

type diagPosition struct {
	Line int `json:"line"`
	Col  int `json:"col"`
}

type diagRange struct {
	Start diagPosition  `json:"start"`
	End   *diagPosition `json:"end,omitempty"`
}

type diagFix struct {
	Available bool   `json:"available"`
	Kind      string `json:"kind"`
}

type diagnostic struct {
	Tool     string     `json:"tool"`
	ID       string     `json:"id,omitempty"`
	Severity string     `json:"severity"`
	Message  string     `json:"message"`
	File     string     `json:"file,omitempty"`
	Range    *diagRange `json:"range,omitempty"`
	URL      string     `json:"url,omitempty"`
	Fix      *diagFix   `json:"fix,omitempty"`
}

var severityRank = map[string]int{"error": 0, "warning": 1, "info": 2, "hint": 3}

func severityOr(severity string, fallback string) string {
	switch severity {
	case "error", "warning", "info", "hint":
		return severity
	}
	return fallback
}

// normalizeDiagnostics：去重键 `(tool, file, line, col, id)`，排序
// `file → line → col → severity → id`（与 Rust `normalize_diagnostics` 一致）。
func normalizeDiagnostics(items []diagnostic) []diagnostic {
	seen := make(map[string]struct{}, len(items))
	deduped := make([]diagnostic, 0, len(items))
	for _, item := range items {
		key := fmt.Sprintf("%s\x00%s\x00%d\x00%d\x00%s", item.Tool, item.File, startLine(item), startCol(item), item.ID)
		if _, exists := seen[key]; exists {
			continue
		}
		seen[key] = struct{}{}
		deduped = append(deduped, item)
	}
	sort.SliceStable(deduped, func(i, j int) bool {
		left, right := deduped[i], deduped[j]
		if left.File != right.File {
			return left.File < right.File
		}
		if startLine(left) != startLine(right) {
			return startLine(left) < startLine(right)
		}
		if startCol(left) != startCol(right) {
			return startCol(left) < startCol(right)
		}
		if severityRank[left.Severity] != severityRank[right.Severity] {
			return severityRank[left.Severity] < severityRank[right.Severity]
		}
		return left.ID < right.ID
	})
	return deduped
}

func startLine(item diagnostic) int {
	if item.Range == nil {
		return 0
	}
	return item.Range.Start.Line
}

func startCol(item diagnostic) int {
	if item.Range == nil {
		return 0
	}
	return item.Range.Start.Col
}

type lintFileEntry struct {
	FilePath string `json:"filePath"`
	Errors   int    `json:"errors"`
	Warnings int    `json:"warnings"`
}

type lintResult struct {
	Adaptor        string          `json:"adaptor"`
	Implementation string          `json:"implementation"`
	Errors         int             `json:"errors"`
	Warnings       int             `json:"warnings"`
	Files          []lintFileEntry `json:"files"`
	Diagnostics    []diagnostic    `json:"diagnostics"`
	Message        string          `json:"message,omitempty"`
	Tool           string          `json:"tool,omitempty"`
}

// buildLintResult：`files` 由 `diagnostics` 派生，保证两个字段永不失配。
func buildLintResult(adaptor string, implementation string, items []diagnostic, message string) lintResult {
	normalized := normalizeDiagnostics(items)
	if normalized == nil {
		normalized = []diagnostic{}
	}
	order := make([]string, 0, len(normalized))
	counts := make(map[string]*lintFileEntry, len(normalized))
	for _, item := range normalized {
		filePath := item.File
		if filePath == "" {
			filePath = "."
		}
		entry, exists := counts[filePath]
		if !exists {
			entry = &lintFileEntry{FilePath: filePath}
			counts[filePath] = entry
			order = append(order, filePath)
		}
		if item.Severity == "error" {
			entry.Errors++
		} else {
			entry.Warnings++
		}
	}
	files := make([]lintFileEntry, 0, len(order))
	errors, warnings := 0, 0
	for _, filePath := range order {
		entry := counts[filePath]
		errors += entry.Errors
		warnings += entry.Warnings
		files = append(files, *entry)
	}
	return lintResult{
		Adaptor:        adaptor,
		Implementation: implementation,
		Errors:         errors,
		Warnings:       warnings,
		Files:          files,
		Diagnostics:    normalized,
		Message:        message,
		Tool:           adaptor,
	}
}

// ---------------------------------------------------------------------------
// 协议输出
// ---------------------------------------------------------------------------

var (
	writeMu sync.Mutex
	seqMu   sync.Mutex
	seq     int
)

// logToStderr：所有非协议输出必须走 stderr（§3.1 约定 1）。
func logToStderr(format string, args ...any) {
	fmt.Fprintf(os.Stderr, format+"\n", args...)
}

func nextSeq() int {
	seqMu.Lock()
	defer seqMu.Unlock()
	seq++
	return seq
}

func emit(envelope map[string]any) {
	writeMu.Lock()
	defer writeMu.Unlock()
	payload, err := json.Marshal(envelope)
	if err != nil {
		logToStderr("failed to encode envelope: %v", err)
		return
	}
	os.Stdout.Write(payload)
	os.Stdout.Write([]byte("\n"))
	os.Stdout.Sync()
}

func emitEvent(method string, params any, requestID string) {
	if requestID == "" {
		requestID = "bridge"
	}
	emit(map[string]any{
		"type":      "event",
		"requestId": requestID,
		"payload":   map[string]any{"method": method, "params": params},
	})
}

func emitDiag(requestID string, text string, stream string, channel string, level string, tool string) {
	params := map[string]any{
		"source":  "runtime",
		"stream":  stream,
		"channel": channel,
		"level":   level,
		"text":    text,
		"seq":     nextSeq(),
		"ts":      time.Now().UnixMilli(),
	}
	if tool != "" {
		params["source"] = "tool"
		params["tool"] = tool
	}
	emitEvent("event.diag", params, requestID)
}

type bridgeError struct {
	Code    string `json:"code"`
	Message string `json:"message"`
	Data    any    `json:"data"`
}

func emitResponse(requestID string, result any, failure *bridgeError) {
	emit(map[string]any{
		"type": "response",
		"payload": map[string]any{
			"id":     requestID,
			"result": result,
			"error":  failure,
		},
	})
}

// ---------------------------------------------------------------------------
// 工具执行 + 流式劫持（§7.2）
// ---------------------------------------------------------------------------

type runOutcome struct {
	exitCode int
	stdout   string
	stderr   string
	err      error
}

// runWithCapture 执行命令并把每一行实时转成 `event.diag`，同时完整保留原始输出
// 供解析器使用（诊断需要整段文本，只留尾巴会丢掉除最后一条以外的错误）。
func runWithCapture(requestID string, cwd string, argv []string, tool string, channel string) runOutcome {
	if len(argv) == 0 {
		return runOutcome{exitCode: 1, err: fmt.Errorf("empty command")}
	}
	command := exec.Command(argv[0], argv[1:]...)
	command.Dir = cwd
	command.Env = append(os.Environ(), "FORCE_COLOR=0")

	stdoutPipe, err := command.StdoutPipe()
	if err != nil {
		return runOutcome{exitCode: 1, err: err}
	}
	stderrPipe, err := command.StderrPipe()
	if err != nil {
		return runOutcome{exitCode: 1, err: err}
	}
	if err := command.Start(); err != nil {
		// 工具不存在 / 不可执行：调用方据此决定 fallback。
		return runOutcome{exitCode: 127, err: err}
	}

	var stdoutBuf, stderrBuf strings.Builder
	var bufMu sync.Mutex
	var wg sync.WaitGroup
	pump := func(reader io.Reader, stream string, sink *strings.Builder) {
		defer wg.Done()
		scanner := bufio.NewScanner(reader)
		// Scanner 默认 64KB 上限：Go 编译器/链接器的长行会超限，直接放到 1MB。
		scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)
		for scanner.Scan() {
			line := scanner.Text()
			bufMu.Lock()
			sink.WriteString(line)
			sink.WriteString("\n")
			bufMu.Unlock()
			if strings.TrimSpace(line) == "" {
				continue
			}
			level := "info"
			if stream == "stderr" && looksLikeError(line) {
				level = "error"
			}
			emitDiag(requestID, line, stream, channel, level, tool)
		}
	}
	wg.Add(2)
	go pump(stdoutPipe, "stdout", &stdoutBuf)
	go pump(stderrPipe, "stderr", &stderrBuf)
	wg.Wait()

	exitCode := 0
	if err := command.Wait(); err != nil {
		if exitError, ok := err.(*exec.ExitError); ok {
			exitCode = exitError.ExitCode()
		} else {
			return runOutcome{exitCode: 1, stdout: stdoutBuf.String(), stderr: stderrBuf.String(), err: err}
		}
	}
	return runOutcome{exitCode: exitCode, stdout: stdoutBuf.String(), stderr: stderrBuf.String()}
}

// looksLikeError：stderr 不再一律当 warning（§7.2 禁止事项 1）。
func looksLikeError(line string) bool {
	trimmed := strings.TrimSpace(line)
	if strings.HasPrefix(trimmed, "#") {
		// `# example.com/demo` 是 go build 的包标题，不是错误。
		return false
	}
	lower := strings.ToLower(trimmed)
	return strings.Contains(lower, "error") ||
		strings.Contains(lower, "cannot") ||
		strings.Contains(lower, "undefined") ||
		strings.Contains(lower, "failed")
}

// ---------------------------------------------------------------------------
// 解析器（与 ts/packages/node-bridge/src/plugins/lint/parsers.ts 逐字段对齐）
// ---------------------------------------------------------------------------

// `file.go:line:col: message` —— 贪婪匹配前缀，因此能正确处理
// Windows 盘符（`C:\a\b.go:3:2:`）和带空格的路径。
var locationLine = regexp.MustCompile(`^(.*):(\d+):(\d+):\s*(.*)$`)

func parseGoLocationText(tool string, severity string, output string) []diagnostic {
	items := make([]diagnostic, 0)
	scanner := bufio.NewScanner(strings.NewReader(output))
	scanner.Buffer(make([]byte, 0, 64*1024), 1024*1024)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		match := locationLine.FindStringSubmatch(line)
		if match == nil {
			continue
		}
		lineNumber, err := strconv.Atoi(match[2])
		if err != nil {
			continue
		}
		col, err := strconv.Atoi(match[3])
		if err != nil {
			continue
		}
		message := strings.TrimSpace(match[4])
		if message == "" {
			continue
		}
		items = append(items, diagnostic{
			Tool:     tool,
			Severity: severity,
			Message:  message,
			File:     match[1],
			Range:    &diagRange{Start: diagPosition{Line: lineNumber, Col: col}},
		})
	}
	return items
}

type golangciPosition struct {
	Filename string `json:"Filename"`
	Line     int    `json:"Line"`
	Column   int    `json:"Column"`
}

type golangciIssue struct {
	FromLinter  string           `json:"FromLinter"`
	Text        string           `json:"Text"`
	Severity    string           `json:"Severity"`
	Pos         golangciPosition `json:"Pos"`
	Replacement *string          `json:"Replacement"`
}

// parseGolangci：旧版直接是数组，新版是 `{"Issues":[...]}`，两种都认。
func parseGolangci(output string) []diagnostic {
	trimmed := strings.TrimSpace(output)
	if trimmed == "" {
		return nil
	}
	var issues []golangciIssue
	if strings.HasPrefix(trimmed, "[") {
		if err := json.Unmarshal([]byte(trimmed), &issues); err != nil {
			return nil
		}
	} else {
		var wrapper struct {
			Issues []golangciIssue `json:"Issues"`
		}
		if err := json.Unmarshal([]byte(trimmed), &wrapper); err != nil {
			return nil
		}
		issues = wrapper.Issues
	}
	items := make([]diagnostic, 0, len(issues))
	for _, issue := range issues {
		if issue.Pos.Filename == "" {
			continue
		}
		var fix *diagFix
		if issue.Replacement != nil && *issue.Replacement != "" {
			fix = &diagFix{Available: true, Kind: "safe"}
		}
		items = append(items, diagnostic{
			Tool:     "golangci-lint",
			ID:       issue.FromLinter,
			Severity: severityOr(issue.Severity, "error"),
			Message:  issue.Text,
			File:     issue.Pos.Filename,
			Range:    &diagRange{Start: diagPosition{Line: issue.Pos.Line, Col: issue.Pos.Column}},
			Fix:      fix,
		})
	}
	return items
}

// ---------------------------------------------------------------------------
// 请求处理
// ---------------------------------------------------------------------------

func decodeParams(raw json.RawMessage) map[string]any {
	params := make(map[string]any)
	if len(raw) > 0 {
		if err := json.Unmarshal(raw, &params); err != nil {
			return make(map[string]any)
		}
	}
	return params
}

func paramString(params map[string]any, key string, fallback string) string {
	if value, ok := params[key].(string); ok && value != "" {
		return value
	}
	return fallback
}

func paramStrings(params map[string]any, key string) []string {
	raw, ok := params[key].([]any)
	if !ok {
		return nil
	}
	values := make([]string, 0, len(raw))
	for _, item := range raw {
		if text, ok := item.(string); ok {
			values = append(values, text)
		}
	}
	return values
}

func resolveTool(name string) (string, bool) {
	path, err := exec.LookPath(name)
	if err != nil {
		return "", false
	}
	return path, true
}

func missingToolResult(adaptor string) lintResult {
	return buildLintResult(adaptor, "fallback", nil,
		fmt.Sprintf("%s is not installed or not on PATH", adaptor))
}

// failedLintResult：工具**执行失败**（不是「没问题」）必须是一条可见诊断。
//
// 否则 `lan lint` 会因为 errors=0 而以 0 退出、报告里什么都不显示 —— 用户既看不到
// 失败原因，也拿不到非零退出码。与 TS `failedRuntimeResult` 的语义一致（errors=1）。
func failedLintResult(adaptor string, message string) lintResult {
	message = strings.TrimSpace(message)
	if message == "" {
		message = adaptor + " failed"
	}
	return buildLintResult(adaptor, "runtime", []diagnostic{{
		Tool:     adaptor,
		Severity: "error",
		Message:  truncateRunes(message, 400),
	}}, "")
}

// truncateRunes：按字符截断（消息可能含中文，按字节切会切出半个字符）。
func truncateRunes(text string, limit int) string {
	runes := []rune(text)
	if len(runes) <= limit {
		return text
	}
	return string(runes[:limit]) + "…"
}

func handleLint(adaptor string, params map[string]any, requestID string) lintResult {
	cwd := paramString(params, "cwd", currentDir())
	args := paramStrings(params, "args")

	emitDiag(requestID, "running "+adaptor, "stdout", "lint", "info", adaptor)

	switch adaptor {
	case "go-vet", "govet":
		// 别名 `go-vet` / `govet` 都接受（§11 的 `lint.govet` 与 lintTools 的 `go-vet`）。
		adaptor = "go-vet"
		if _, ok := resolveTool("go"); !ok {
			return missingToolResult(adaptor)
		}
		if len(args) == 0 {
			args = []string{"vet", "./..."}
		}
		outcome := runOnce(cwd, append([]string{"go"}, args...))
		if outcome.err != nil {
			return failedLintResult(adaptor, outcome.err.Error())
		}
		items := parseGoLocationText("go-vet", "error", outcome.stdout+"\n"+outcome.stderr)
		// `go vet` 有诊断时退出码就是 1；退出码非 0 却解析不出任何诊断，
		// 说明工具本身没跑起来（缺 go.sum、构建失败…），必须当成失败汇报。
		if len(items) == 0 && outcome.exitCode != 0 {
			return failedLintResult(adaptor, outcome.stderr+"\n"+outcome.stdout)
		}
		return buildLintResult(adaptor, "runtime", items, "")

	case "golangci-lint":
		binary, ok := resolveTool("golangci-lint")
		if !ok {
			return missingToolResult(adaptor)
		}
		if len(args) == 0 {
			args = []string{"run", "--out-format", "json", "./..."}
		}
		outcome := runOnce(cwd, append([]string{binary}, args...))
		if outcome.err != nil {
			return failedLintResult(adaptor, outcome.err.Error())
		}
		items := parseGolangci(outcome.stdout)
		if len(items) == 0 {
			items = parseGolangci(outcome.stderr)
		}
		if len(items) == 0 && outcome.exitCode != 0 {
			return failedLintResult(adaptor, outcome.stderr+"\n"+outcome.stdout)
		}
		return buildLintResult(adaptor, "runtime", items, "")

	default:
		return lintResult{
			Adaptor:        adaptor,
			Implementation: "fallback",
			Files:          []lintFileEntry{},
			Diagnostics:    []diagnostic{},
			Message:        fmt.Sprintf("unsupported lint adaptor: %s", adaptor),
			Tool:           adaptor,
		}
	}
}

func currentDir() string {
	cwd, err := os.Getwd()
	if err != nil {
		return "."
	}
	return cwd
}

// runOnce：不发 `event.diag`，只收集输出（lint 场景下逐行日志价值低，
// 结果由 `Diagnostic` 表达；需要保真日志的是 build/dev）。
func runOnce(cwd string, argv []string) runOutcome {
	if len(argv) == 0 {
		return runOutcome{exitCode: 1, err: fmt.Errorf("empty command")}
	}
	command := exec.Command(argv[0], argv[1:]...)
	command.Dir = cwd
	command.Env = append(os.Environ(), "FORCE_COLOR=0")
	var stdoutBuf, stderrBuf strings.Builder
	command.Stdout = &stdoutBuf
	command.Stderr = &stderrBuf
	// `go vet` 发现问题时退出码非 0，这是正常结果而不是执行失败。
	if err := command.Start(); err != nil {
		return runOutcome{exitCode: 127, err: err}
	}
	err := command.Wait()
	exitCode := 0
	if err != nil {
		exitError, ok := err.(*exec.ExitError)
		if !ok {
			return runOutcome{exitCode: 1, stdout: stdoutBuf.String(), stderr: stderrBuf.String(), err: err}
		}
		exitCode = exitError.ExitCode()
	}
	return runOutcome{exitCode: exitCode, stdout: stdoutBuf.String(), stderr: stderrBuf.String()}
}

func handleBuild(params map[string]any, requestID string) any {
	cwd := paramString(params, "cwd", currentDir())
	outputDir := paramString(params, "outputDir", "bin/app")
	command := paramStrings(params, "command")
	if len(command) == 0 {
		command = []string{"go", "build", "-o", outputDir, "."}
	}

	outcome := runWithCapture(requestID, cwd, command, "go", "build")
	items := parseGoLocationText("go", "error", outcome.stderr)
	items = append(items, parseGoLocationText("go", "error", outcome.stdout)...)
	items = normalizeDiagnostics(items)

	success := outcome.err == nil && outcome.exitCode == 0
	message := ""
	if !success {
		message = strings.TrimSpace(lastNonEmptyLine(outcome.stderr))
		if message == "" {
			message = strings.TrimSpace(lastNonEmptyLine(outcome.stdout))
		}
		if message == "" && outcome.err != nil {
			message = outcome.err.Error()
		}
		if message == "" {
			message = fmt.Sprintf("go build exited with code %d", outcome.exitCode)
		}
	}
	result := map[string]any{
		"success":     success,
		"tool":        "go",
		"runtime":     "go",
		"action":      "build",
		"bridge":      bridgeName,
		"exitCode":    outcome.exitCode,
		"outputDir":   outputDir,
		"diagnostics": items,
		"command":     command,
	}
	if !success {
		result["error"] = message
	}
	return result
}

func lastNonEmptyLine(text string) string {
	lines := strings.Split(text, "\n")
	for index := len(lines) - 1; index >= 0; index-- {
		if trimmed := strings.TrimSpace(lines[index]); trimmed != "" {
			return trimmed
		}
	}
	return ""
}

func handshakeResponse(request map[string]any) map[string]any {
	protocol := protocolVersion
	if value, ok := request["protocolVersion"].(string); ok && value != "" {
		protocol = value
	}
	return map[string]any{
		"protocolVersion":     protocol,
		"bridgeName":          bridgeName,
		"methods":             bridgeMethods,
		"events":              bridgeEvents,
		"heartbeatIntervalMs": heartbeatMs,
		"maxPendingEvents":    maxPendingEvents,
		"failureStrategy":     failureStrategy,
	}
}

func handleRequest(envelope map[string]any) bool {
	payload, _ := envelope["payload"].(map[string]any)
	requestID, _ := payload["id"].(string)
	method, _ := payload["method"].(string)
	var params map[string]any
	if raw, ok := payload["params"]; ok && raw != nil {
		if encoded, err := json.Marshal(raw); err == nil {
			params = decodeParams(encoded)
		}
	}
	if params == nil {
		params = make(map[string]any)
	}

	switch {
	case method == "bridge.ping":
		emitResponse(requestID, map[string]any{"ok": true, "bridgeName": bridgeName, "version": version}, nil)
		return true
	case method == "bridge.heartbeat":
		emitResponse(requestID, map[string]any{"ok": true, "ts": time.Now().UnixMilli()}, nil)
		return true
	case method == "bridge.shutdown":
		emitEvent("event.shutdown", map[string]any{"reason": "shutdown"}, requestID)
		emitResponse(requestID, map[string]any{"ok": true}, nil)
		return false
	case strings.HasPrefix(method, "lint."):
		adaptor := strings.TrimPrefix(method, "lint.")
		emitEvent("event.lint_start", map[string]any{"adaptors": []string{adaptor}}, requestID)
		result := handleLint(adaptor, params, requestID)
		emitEvent("event.lint_result", map[string]any{
			"adaptor":        adaptor,
			"errors":         result.Errors,
			"warnings":       result.Warnings,
			"implementation": result.Implementation,
		}, requestID)
		emitResponse(requestID, result, nil)
		return true
	case method == "build.go":
		result := handleBuild(params, requestID)
		emitResponse(requestID, result, nil)
		return true
	default:
		emitResponse(requestID, nil, &bridgeError{
			Code:    "E_UNSUPPORTED",
			Message: "unsupported method: " + method,
			Data:    nil,
		})
		return true
	}
}

func serve() int {
	emitEvent("event.ready", map[string]any{
		"bridgeName": bridgeName,
		"methods":    bridgeMethods,
		"pid":        os.Getpid(),
	}, "")

	reader := bufio.NewReaderSize(os.Stdin, 1024*1024)
	for {
		line, err := reader.ReadString('\n')
		if len(line) == 0 && err != nil {
			// stdin EOF：父进程消失，自杀（§3.1 约定 4）。
			return 0
		}
		trimmed := strings.TrimSpace(line)
		if trimmed == "" {
			if err != nil {
				return 0
			}
			continue
		}
		var envelope map[string]any
		if decodeErr := json.Unmarshal([]byte(trimmed), &envelope); decodeErr != nil {
			logToStderr("failed to parse envelope: %v", decodeErr)
			if err != nil {
				return 0
			}
			continue
		}
		switch envelope["type"] {
		case "handshake":
			request, _ := envelope["payload"].(map[string]any)
			if request == nil {
				request = make(map[string]any)
			}
			emit(map[string]any{"type": "handshake", "payload": handshakeResponse(request)})
		case "request":
			if !handleRequest(envelope) {
				return 0
			}
		case "host_response":
			// 本 bridge 不发起 host RPC，忽略。
		default:
			logToStderr("unsupported envelope type: %v", envelope["type"])
		}
		if err != nil {
			return 0
		}
	}
}

func main() {
	if len(os.Args) > 1 && os.Args[1] == "--version" {
		fmt.Fprintf(os.Stderr, "%s %s\n", bridgeName, version)
		os.Exit(0)
	}
	os.Exit(serve())
}
