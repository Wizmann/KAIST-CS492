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
// 一个简单的线程池
// ===================================
struct ThreadPool {
    tx: mpsc::Sender<Message>, // 用于发送任务到线程池
    workers: Vec<thread::JoinHandle<()>>, // 存储工作线程的句柄
}

// 消息类型，代表任务和终止信号
enum Message {
    Job(Box<dyn FnOnce() + Send + 'static>), // 存储需要执行的任务
    Terminate, // 终止信号
}

impl ThreadPool {
    // 创建新的线程池，`size` 为线程数
    fn new(size: usize) -> Self {
        assert!(size > 0); // 确保线程池大小大于 0

        let (tx, rx) = mpsc::channel::<Message>(); // 创建消息通道
        let rx = Arc::new(Mutex::new(rx)); // 使用 Arc 和 Mutex 保证多线程安全

        let mut workers = Vec::with_capacity(size); // 分配空间给工作线程

        // 创建并启动指定数量的工作线程
        for id in 0..size {
            let rx = Arc::clone(&rx); // 克隆 Arc，确保线程间共享

            // 创建一个新的工作线程
            let handle = thread::Builder::new()
                .name(format!("worker-{id}"))
                .spawn(move || loop {
                    match rx.lock().unwrap().recv() { // 获取任务
                        Ok(Message::Job(job)) => job(), // 执行任务
                        Ok(Message::Terminate) | Err(_) => break, // 终止信号，退出循环
                    }
                })
                .expect("spawn worker"); // 创建线程失败时 panic

            workers.push(handle); // 将线程句柄保存到 workers 中
        }

        ThreadPool { tx, workers } // 返回线程池实例
    }

    // 向线程池发送终止信号
    fn shutdown(&mut self) {
        // 向每个 worker 发送一个终止信号
        for _ in 0..self.workers.len() {
            let _ = self.tx.send(Message::Terminate);
        }

        // 等待每个工作线程结束
        while let Some(h) = self.workers.pop() {
            let _ = h.join();
        }
    }

    // 向线程池发送任务
    fn execute<F>(&self, job: F)
    where
        F: FnOnce() + Send + 'static, // 任务类型，必须实现 FnOnce，并且能够发送到其他线程
    {
        let _ = self.tx.send(Message::Job(Box::new(job))); // 发送任务
    }
}

// 在 ThreadPool 被销毁时，清理资源
impl Drop for ThreadPool {
    fn drop(&mut self) {
        // 向每个 worker 发送一个终止信号
        for _ in &self.workers {
            let _ = self.tx.send(Message::Terminate);
        }

        // 等待每个工作线程退出
        while let Some(h) = self.workers.pop() {
            let _ = h.join();
        }
    }
}

// ===================================
// HTTP 服务器（阻塞 I/O）
// ===================================
fn main() -> std::io::Result<()> {
    // 绑定 TCP 监听器，监听 7878 端口
    let listener = TcpListener::bind(("0.0.0.0", 7878))?;
    listener.set_nonblocking(true)?; // 设置非阻塞，便于后续轮询
    println!("listening on http://0.0.0.0:7878 (GET /path → echo /path, task = sleep 1s)");

    let mut pool = ThreadPool::new(num_cpus()); // 创建线程池，线程数为 CPU 核心数

    // 线程安全的缓存：记录访问路径和次数
    let cache = Arc::new(Mutex::new(HashMap::<String, usize>::new()));

    // ---- 优雅关机：信号 + 原子标记 ----
    let shutdown = Arc::new(AtomicBool::new(false));
    // 注册信号处理器，当收到 SIGINT 或 SIGTERM 时，设置关机标记
    signal_flag::register(SIGINT, Arc::clone(&shutdown))
        .expect("register SIGINT");
    signal_flag::register(SIGTERM, Arc::clone(&shutdown))
        .expect("register SIGTERM");

    // 主循环：轮询 TCP 连接并检查关机标记
    loop {
        if shutdown.load(Ordering::Relaxed) { // 检查是否需要关机
            eprintln!("\nShutting down: stop accepting new connections…");
            break;
        }

        match listener.accept() { // 接受新的 TCP 连接
            Ok((stream, _addr)) => {
                let cache = Arc::clone(&cache); // 克隆缓存的 Arc
                pool.execute(move || { // 执行任务：处理连接
                    if let Err(e) = handle_conn(stream, cache) {
                        eprintln!("conn error: {e}");
                    }
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // 如果没有新的连接，稍微休息，避免忙等
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                eprintln!("accept error: {e}");
                thread::sleep(Duration::from_millis(100)); // 错误继续循环
            }
        }
    }

    pool.shutdown(); // 关停线程池，等待工作线程退出

    // 最后打印缓存统计信息
    print_cache_stats(&cache);

    Ok(())
}

// 打印缓存访问统计信息
fn print_cache_stats(cache: &Arc<Mutex<HashMap<String, usize>>>) {
    let mut items: Vec<(String, usize)> = {
        let map = cache.lock().unwrap(); // 锁定缓存
        map.iter().map(|(k, &v)| (k.clone(), v)).collect() // 克隆数据避免锁定期间修改
    };
    items.sort_by(|a, b| b.1.cmp(&a.1)); // 按访问次数降序排序

    println!("\n==== Access Stats ====");
    if items.is_empty() {
        println!("(empty)");
        return;
    }
    for (path, cnt) in items {
        println!("{cnt:>6}  {path}");
    }
}

// 处理 TCP 连接：读取请求并返回响应
fn handle_conn(mut stream: TcpStream, cache: Arc<Mutex<HashMap<String, usize>>>) -> std::io::Result<()> {
    let mut buf = [0u8; 8192]; // 缓存请求数据
    let mut n = 0usize;

    // 循环读取直到找到 CRLF 或缓冲区满
    loop {
        if n == buf.len() {
            break;
        }
        let readn = stream.read(&mut buf[n..])?; // 读取数据到缓存
        if readn == 0 {
            return Ok(()); // 客户端关闭连接
        }
        n += readn;

        if let Some(line_end) = find_crlf(&buf[..n]) { // 找到请求行的结束
            let line = &buf[..line_end]; // 获取请求行
            let path = parse_path_from_request_line(line).unwrap_or_else(|| "/".to_string());

            // ========= cache 逻辑 =========
            // 如果缓存命中，直接返回路径和 emoji
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

            // 未命中缓存，模拟任务处理（延迟 1 秒）
            thread::sleep(Duration::from_secs(1));

            // 插入或更新缓存
            {
                let mut map = cache.lock().unwrap();
                let entry = map.entry(path.clone()).or_insert(0);
                *entry += 1;
            }

            // 返回路径作为响应
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

    // 如果没有解析到有效请求行，返回 400 错误
    let resp = b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
    stream.write_all(resp)?;
    stream.flush()?;
    Ok(())
}

// 查找 CRLF（\r\n）的位置
fn find_crlf(buf: &[u8]) -> Option<usize> {
    for i in 0..buf.len().saturating_sub(1) {
        if buf[i] == b'\r' && buf[i + 1] == b'\n' {
            return Some(i); // 返回 CRLF 结束的位置
        }
    }
    None
}

// 解析请求行中的路径部分
fn parse_path_from_request_line(line: &[u8]) -> Option<String> {
    let mut parts = line.split(|&b| b == b' ');
    let method = parts.next()?;
    if method != b"GET" {
        return Some("/".to_string()); // 只处理 GET 请求，其他的返回 "/"
    }
    let path = parts.next().unwrap_or(b"/");
    Some(String::from_utf8_lossy(path).into_owned())
}

// 获取 CPU 核心数
fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .max(2) // 至少保证有两个线程
}

