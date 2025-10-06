use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook::flag as signal_flag;

// ===================================
// 一个简单的线程池（唯一的池）
// ===================================
struct ThreadPool {
    tx: mpsc::Sender<Message>,
    workers: Vec<thread::JoinHandle<()>>,
}

enum Message {
    Job(Box<dyn FnOnce() + Send + 'static>),
    Terminate,
}

impl ThreadPool {
    fn new(size: usize) -> Self {
        assert!(size > 0);
        let (tx, rx) = mpsc::channel::<Message>();
        let rx = Arc::new(Mutex::new(rx));
        let mut workers = Vec::with_capacity(size);
        for id in 0..size {
            let rx = Arc::clone(&rx);
            let handle = thread::Builder::new()
                .name(format!("worker-{id}"))
                .spawn(move || loop {
                    match rx.lock().unwrap().recv() {
                        Ok(Message::Job(job)) => job(),
                        Ok(Message::Terminate) | Err(_) => break,
                    }
                })
                .expect("spawn worker");
            workers.push(handle);
        }
        ThreadPool { tx, workers }
    }

    /// 显式优雅关机：广播 Terminate，并逐个 join
    fn shutdown(&mut self) {
        // 向每个 worker 发送一个 Terminate（和 worker 数量一致）
        for _ in 0..self.workers.len() {
            let _ = self.tx.send(Message::Terminate);
        }
        // 逐个 join，等待在途任务跑完并收到 Terminate 后退出
        while let Some(h) = self.workers.pop() {
            let _ = h.join();
        }
    }

    fn execute<F>(&self, job: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let _ = self.tx.send(Message::Job(Box::new(job)));
    }
}

impl Drop for ThreadPool {
    fn drop(&mut self) {
        for _ in &self.workers {
            let _ = self.tx.send(Message::Terminate);
        }
        while let Some(h) = self.workers.pop() {
            let _ = h.join();
        }
    }
}

// ===================================
// HTTP 服务器（阻塞 I/O）
// ===================================
fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind(("0.0.0.0", 7878))?;
    listener.set_nonblocking(true)?; // 改为非阻塞，便于轮询关机标记
    println!("listening on http://0.0.0.0:7878 (GET /path → echo /path, task = sleep 3s)");

    let mut pool = ThreadPool::new(num_cpus());

    // 线程安全的 cache：path -> 访问次数
    let cache = Arc::new(Mutex::new(HashMap::<String, usize>::new()));

    // ---- 优雅关机：信号 + 原子标记 ----
    let shutdown = Arc::new(AtomicBool::new(false));
    // 在收到 Ctrl+C (SIGINT) 或 kill (SIGTERM) 时，将标记设为 true
    signal_flag::register(SIGINT, Arc::clone(&shutdown))
        .expect("register SIGINT");
    signal_flag::register(SIGTERM, Arc::clone(&shutdown))
        .expect("register SIGTERM");

    // 主循环：轮询 accept，检查关机标记
    loop {
        if shutdown.load(Ordering::Relaxed) {
            eprintln!("\nShutting down: stop accepting new connections…");
            break;
        }

        match listener.accept() {
            Ok((stream, _addr)) => {
                let cache = Arc::clone(&cache);
                pool.execute(move || {
                    if let Err(e) = handle_conn(stream, cache) {
                        eprintln!("conn error: {e}");
                    }
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // 没有新连接，稍微歇一会避免忙等
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                eprintln!("accept error: {e}");
                // 致命错误可以选择 break；这里选择继续循环
                thread::sleep(Duration::from_millis(100));
            }
        }
    }

    // 跳出循环：ThreadPool 在离开作用域时会 Drop，
    // 向 worker 广播 Terminate 并逐个 join，等待在途任务自然完成。
    pool.shutdown();

    // 最后打印统计
    print_cache_stats(&cache);

    Ok(())
}


fn print_cache_stats(cache: &Arc<Mutex<HashMap<String, usize>>>) {
    let mut items: Vec<(String, usize)> = {
        let map = cache.lock().unwrap();
        map.iter().map(|(k, &v)| (k.clone(), v)).collect()
    };
    items.sort_by(|a, b| b.1.cmp(&a.1));

    println!("\n==== Access Stats ====");
    if items.is_empty() {
        println!("(empty)");
        return;
    }
    for (path, cnt) in items {
        println!("{cnt:>6}  {path}");
    }
}

fn handle_conn(mut stream: TcpStream, cache: Arc<Mutex<HashMap<String, usize>>>) -> std::io::Result<()> {
    // 读取最多 8KB（只为解析第一行）
    let mut buf = [0u8; 8192];
    let mut n = 0usize;

    // 循环读直到发现 CRLF（第一行结束）或缓冲区满
    loop {
        if n == buf.len() {
            break;
        }
        let readn = stream.read(&mut buf[n..])?;
        if readn == 0 {
            return Ok(()); // 客户端关闭
        }
        n += readn;

        if let Some(line_end) = find_crlf(&buf[..n]) {
            let line = &buf[..line_end]; // 不含 CRLF
            let path = parse_path_from_request_line(line).unwrap_or_else(|| "/".to_string());

            // ========= cache 逻辑 =========
            // 命中：加 1、直接返回路径 + emoji（避免持锁做 I/O）
            let mut hit = false;
            {
                let mut map = cache.lock().unwrap();
                if let Some(cnt) = map.get_mut(&path) {
                    *cnt += 1;
                    hit = true;
                }
            }

            if hit {
                let body = format!("{path} 🙂");
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.as_bytes().len()
                );
                stream.write_all(header.as_bytes())?;
                stream.write_all(body.as_bytes())?;
                stream.flush()?;
                return Ok(());
            }

            // 未命中：执行“重任务”
            thread::sleep(Duration::from_secs(3));

            // 插入 cache（计数=1）
            {
                let mut map = cache.lock().unwrap();
                // 如果期间有其他请求已插入该 path，则这里选择加 1（幂等性不严格要求）
                let entry = map.entry(path.clone()).or_insert(0);
                if *entry == 0 {
                    *entry = 1;
                } else {
                    *entry += 1;
                }
            }

            // 回写响应（回显路径）
            let body = path.as_bytes();
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(header.as_bytes())?;
            stream.write_all(body)?;
            stream.flush()?;
            return Ok(());
        }
    }

    // 没解析到起始行，返回 400
    let resp = b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
    stream.write_all(resp)?;
    stream.flush()?;
    Ok(())
}

// 找到 CRLF 的位置（返回 '\r' 的索引）
fn find_crlf(buf: &[u8]) -> Option<usize> {
    for i in 0..buf.len().saturating_sub(1) {
        if buf[i] == b'\r' && buf[i + 1] == b'\n' {
            return Some(i);
        }
    }
    None
}

/// 解析 "GET /foo/bar?x=1 HTTP/1.1" → "/foo/bar?x=1"
fn parse_path_from_request_line(line: &[u8]) -> Option<String> {
    let mut parts = line.split(|&b| b == b' ');
    let method = parts.next()?;
    if method != b"GET" {
        return Some("/".to_string());
    }
    let path = parts.next().unwrap_or(b"/");
    Some(String::from_utf8_lossy(path).into_owned())
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .max(2) // 至少 2 个
}
