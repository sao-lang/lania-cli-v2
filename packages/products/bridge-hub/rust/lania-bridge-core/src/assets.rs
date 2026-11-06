use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// 把内嵌的端点资产（生成物 + 手写 adaptor）落盘到可写目录。
///
/// 不做 dylib 热插拔；子进程端点需要真实文件才能被 `python` / `node` / `go` / `dart`
/// 加载，所以启动时按需写到 workspace 内的临时目录，而不是系统 Temp。
#[derive(Debug, Clone)]
pub struct AssetStore {
    root: PathBuf,
}

impl AssetStore {
    pub fn new(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 写入单个文件，父目录自动创建；内容一致时不重写（避免无谓 mtime 抖动）。
    pub fn stage(&self, relative: &str, contents: &str) -> io::Result<PathBuf> {
        let target = self.root.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let needs_write = match fs::read_to_string(&target) {
            Ok(current) => current.replace("\r\n", "\n") != contents.replace("\r\n", "\n"),
            Err(_) => true,
        };
        if needs_write {
            fs::write(&target, contents)?;
        }
        Ok(target)
    }

    pub fn stage_all(&self, assets: &[(&str, &str)]) -> io::Result<Vec<PathBuf>> {
        assets.iter().map(|(path, body)| self.stage(path, body)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_into_nested_directories() {
        let root = std::env::current_dir().unwrap().join("../../../../../rust/target/.tmp/bridge-assets-core-test");
        let store = AssetStore::new(&root).unwrap();
        let path = store.stage("protocol/gen/py/runtime.py", "print('ok')\n").unwrap();
        assert!(path.exists());
        assert_eq!(fs::read_to_string(&path).unwrap(), "print('ok')\n");
        // 幂等：第二次不重写
        assert!(store.stage("protocol/gen/py/runtime.py", "print('ok')\n").is_ok());
        let _ = fs::remove_dir_all(&root);
    }
}
