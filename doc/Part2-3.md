# KAIST CS492 - 并发编程 · 课程速读（Part2.3 Rust中的安全API - ParkingLot库）

## 基于锁的并发 — ParkingLot 的 API

在这节课中，我们深入了解了 Rust 的 `parking_lot` 库，这个库提供了一些高效且灵活的并发工具，如 `Mutex`、`条件变量`（Conditional Variables）和 `读写锁`（Reader-Writer Lock）。这些 API 可以帮助我们以更加高效、安全的方式进行并发编程。接下来，我们将详细讲解如何使用这些 API。

## 一、`Mutex` 及其用法

在 Rust 中，`Mutex` 用于保护共享数据，确保同一时间只有一个线程可以访问数据。`parking_lot` 库中的 `Mutex` 提供了比标准库 `std::sync::Mutex` 更高效的实现，主要特点是锁的获取与释放更加快速。

### 示例代码：

```rust
use std::sync::{Arc};
use parking_lot::Mutex;
use std::thread;

fn main() {
    // 创建一个被 Mutex 保护的数据值 0，并使用 Arc 包裹它
    let data = Arc::new(Mutex::new(0));

    let mut handles = vec![];

    // 创建 10 个线程，每个线程都会增加数据值
    for _ in 0..10 {
        let data = Arc::clone(&data);  // 克隆 Arc 使数据可以被多个线程共享
        let handle = thread::spawn(move || {
            let mut num = data.lock();  // 获取 Mutex 锁，安全地访问共享数据
            *num += 1;  // 对共享数据进行修改
        });
        handles.push(handle);
    }

    // 等待所有线程完成
    for handle in handles {
        handle.join().unwrap();  // 使用 join 来阻塞直到线程完成
    }

    println!("Result: {}", *data.lock());  // 输出最终的共享数据
}
```

### 关键点：

* **`Arc`**：用于在线程间共享对数据的所有权。
* **`Mutex`**：用于在多个线程之间进行数据访问的互斥锁，保证同一时刻只有一个线程可以修改数据。
* **`lock()`**：用于获取 `Mutex` 的锁，当线程获取锁后，可以安全地对被保护的数据进行修改。

通过 `parking_lot::Mutex`，我们能够高效地进行数据保护，且在锁释放后，其他线程可以继续获取锁。

## 二、`Conditional Variable`（条件变量）

条件变量用于协调多个线程之间的执行顺序。它的主要作用是让线程在等待某个条件满足时不会占用 CPU 资源。条件变量通常与 `Mutex` 一起使用，保证线程在等待条件满足时释放锁，允许其他线程执行。

### 示例代码：

```rust
use parking_lot::{Mutex, Condvar};
use std::sync::Arc;
use std::thread;

fn main() {
    let started = Arc::new(Mutex::new(false));  // 共享的条件变量
    let cvar = Arc::new(Condvar::new());

    let started_clone = Arc::clone(&started);
    let cvar_clone = Arc::clone(&cvar);

    // 启动一个线程来修改条件
    thread::spawn(move || {
        let mut started = started_clone.lock();
        *started = true;
        cvar_clone.notify_all();  // 通知等待的线程
    });

    // 主线程等待条件满足
    let mut started = started.lock();
    while !*started {
        // 如果条件未满足，等待
        cvar.wait(&mut started);  // 等待条件变量，自动释放锁
    }
    println!("Condition met, proceeding!");
}
```

### 关键点：

* **`Condvar`**：条件变量，用于在某些条件未满足时让线程阻塞，直到被其他线程通知。
* **`wait()`**：调用 `wait()` 会释放当前锁，并使线程阻塞，直到接收到通知。
* **`notify_all()`**：通知所有等待的线程，唤醒它们继续执行。

条件变量通过与 `Mutex` 结合使用，提供了一种高效的线程同步机制，避免了线程在等待条件满足时占用过多 CPU 资源。

## 三、`Reader-Writer Lock`（读写锁）

`Reader-Writer Lock` 允许多个线程同时读取共享数据，但只有一个线程可以写数据。`parking_lot` 提供的读写锁实现比标准库中的 `RwLock` 更加高效。

### 示例代码：

```rust
use parking_lot::RwLock;
use std::sync::Arc;
use std::thread;

fn main() {
    let data = Arc::new(RwLock::new(0));  // 使用 RwLock 保护共享数据

    let mut handles = vec![];

    // 创建 10 个线程，模拟读取操作
    for _ in 0..10 {
        let data = Arc::clone(&data);  // 克隆 Arc 以便在多个线程之间共享数据
        let handle = thread::spawn(move || {
            let num = data.read();  // 获取读锁
            println!("Read value: {}", *num);
        });
        handles.push(handle);
    }

    // 创建一个线程进行写操作
    let data = Arc::clone(&data);
    let handle = thread::spawn(move || {
        let mut num = data.write();  // 获取写锁
        *num += 1;
        println!("Written value: {}", *num);
    });
    handles.push(handle);

    // 等待所有线程完成
    for handle in handles {
        handle.join().unwrap();
    }

    println!("Final value: {}", *data.read());  // 最终值
}
```

### 关键点：

* **`RwLock`**：允许多个线程同时读取数据，但写数据时会独占锁。适合读操作远多于写操作的场景。
* **`read()`**：获取读锁，允许多个线程并发读取。
* **`write()`**：获取写锁，确保只有一个线程可以修改数据。

`RwLock` 提供了一种更加高效的锁机制，当数据主要用于读取时，多个线程可以同时读取共享数据，从而提高并发性能。

## 四、总结

在这一节中，我们介绍了 `parking_lot` 库中的几个关键并发 API：

1. **`Mutex`**：用于保护共享数据，确保同一时刻只有一个线程可以访问数据。
2. **`Conditional Variable`**：允许线程在等待某个条件时阻塞，避免占用 CPU 资源。
3. **`Reader-Writer Lock`**：允许多个线程同时读取数据，但只有一个线程可以写数据，适用于读多写少的场景。

这些 API 都提供了安全的操作，并且通过 `parking_lot` 库，我们能够获得比标准库中更高效的锁实现。`parking_lot` 的这些 API 对于高并发环境下的数据保护和线程协调非常有帮助。
