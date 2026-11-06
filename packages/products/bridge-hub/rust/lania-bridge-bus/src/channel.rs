use std::io::{BufRead, BufReader, BufWriter, Write};
use std::process::{ChildStdin, ChildStdout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use lania_bridge_core::{Channel, ChannelError};
use lania_bridge_protocol::{decode_frame, encode_frame, BusFrame, E_BAD_FRAME};

/// 进程内 channel：Rust 端点、CLI 内嵌端点、单元测试走这条。
#[derive(Clone)]
pub struct InProcessChannel {
    tx: Arc<Mutex<mpsc::Sender<BusFrame>>>,
    rx: Arc<Mutex<mpsc::Receiver<BusFrame>>>,
    closed: Arc<AtomicBool>,
}

impl InProcessChannel {
    fn new(tx: mpsc::Sender<BusFrame>, rx: mpsc::Receiver<BusFrame>) -> Self {
        Self {
            tx: Arc::new(Mutex::new(tx)),
            rx: Arc::new(Mutex::new(rx)),
            closed: Arc::new(AtomicBool::new(false)),
        }
    }

    /// 返回 (Hub 侧, 端点侧)。两侧都可读可写，全双工。
    pub fn pair() -> (Self, Self) {
        let (endpoint_to_hub, hub_rx) = mpsc::channel();
        let (hub_to_endpoint, endpoint_rx) = mpsc::channel();
        (
            Self::new(hub_to_endpoint, hub_rx),
            Self::new(endpoint_to_hub, endpoint_rx),
        )
    }

    /// 测试用：模拟对端断开。
    pub fn mark_closed(&self) {
        self.closed.store(true, Ordering::SeqCst);
    }
}

impl Channel for InProcessChannel {
    fn send(&self, frame: BusFrame) -> Result<(), ChannelError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ChannelError::Closed);
        }
        let tx = self.tx.lock().expect("in-process sender poisoned");
        tx.send(frame).map_err(|_| {
            self.closed.store(true, Ordering::SeqCst);
            ChannelError::Closed
        })
    }

    fn try_recv(&self) -> Option<Result<BusFrame, ChannelError>> {
        let rx = self.rx.lock().expect("in-process receiver poisoned");
        match rx.try_recv() {
            Ok(frame) => Some(Ok(frame)),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.closed.store(true, Ordering::SeqCst);
                None
            }
        }
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}

/// stdio JSON 行 channel：任意语言子进程端点接进来。
///
/// 读取在后台线程做（BufReader 是阻塞的），Hub 侧 `try_recv` 只轮询 mpsc，
/// 因此 pump 依旧是非阻塞的同步循环。EOF 或写失败即视为端点断连。
pub struct StdioChannel {
    writer: Mutex<BufWriter<ChildStdin>>,
    inbound: Mutex<mpsc::Receiver<Result<BusFrame, ChannelError>>>,
    closed: Arc<AtomicBool>,
}

impl StdioChannel {
    pub fn new(stdout: ChildStdout, stdin: ChildStdin) -> Self {
        let (tx, inbound) = mpsc::channel();
        let closed = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&closed);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(raw) => {
                        let text = raw.trim();
                        if text.is_empty() {
                            continue;
                        }
                        let item = match decode_frame(text) {
                            Ok(frame) => Ok(frame),
                            Err(err) => Err(ChannelError::Decode(format!("{E_BAD_FRAME}: {err}"))),
                        };
                        if tx.send(item).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            flag.store(true, Ordering::SeqCst);
        });
        Self {
            writer: Mutex::new(BufWriter::new(stdin)),
            inbound: Mutex::new(inbound),
            closed,
        }
    }
}

impl Channel for StdioChannel {
    fn send(&self, frame: BusFrame) -> Result<(), ChannelError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(ChannelError::Closed);
        }
        let line = encode_frame(&frame).map_err(|err| ChannelError::Io(err.to_string()))?;
        let mut writer = self.writer.lock().expect("stdio writer poisoned");
        writer
            .write_all(line.as_bytes())
            .and_then(|_| writer.write_all(b"\n"))
            .and_then(|_| writer.flush())
            .map_err(|err| {
                self.closed.store(true, Ordering::SeqCst);
                ChannelError::Io(err.to_string())
            })
    }

    fn try_recv(&self) -> Option<Result<BusFrame, ChannelError>> {
        self.inbound
            .lock()
            .expect("stdio inbound poisoned")
            .try_recv()
            .ok()
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
}
