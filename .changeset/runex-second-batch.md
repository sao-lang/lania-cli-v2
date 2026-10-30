---
'@lania-cli-v2/cli': minor
---

第二批自管运行时（Java / Kotlin / Scala / Dart）：

- `runex` 与 `lan env` 新增 Java（Temurin）、Kotlin、Scala 3、Dart SDK 的安装 / 切换 / 卸载 / 列举；
- Java 设置 `JAVA_HOME` / `JDK_HOME`，Kotlin/Scala 设置各自的 home，Dart 设置 `DART_SDK`；
- Kotlin / Scala 只安装编译器发行包，运行仍需要 JDK；
- `lan env list` / `info` / `versions` 会自动识别这些语言为 `self-managed`。
