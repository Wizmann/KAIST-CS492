# KAIST CS492 - 并发编程 · 课程速读（Part2.2 Rust中的安全API）

## Rust 中安全 API 的基础

在前一节中，我们讨论了基于锁的并发和如何使用简单的锁来控制共享资源的访问，防止竞态条件和数据不一致。然而，尽管基于锁的并发是一种强大的同步机制，底层的锁 API 往往存在可扩展性差和容易出错的问题。为了解决这些问题，Rust 提供了**安全的 API**，使得用户无需关心锁的具体实现，而可以专注于使用 API 进行并发编程。

## 一、Rust 的并发库概览

Rust 的并发库将潜在的不安全实现封装在安全的 API 中。这意味着用户在使用这些 API 时不需要担心底层实现的细节，只需确保正确使用 API，就可以避免不确定行为或其他潜在的安全问题。Rust 提供的并发库包括：

* **标准库（std）**中的线程、原子引用计数等。
* **ParkingLot**库，提供低级并发特性，如互斥锁（Mutex）和条件变量（Condvar）。
* **Crossbeam**库，提供线程池、跨线程通信（channel）等功能。
* **Rayon**库，提供易于使用的并行处理接口，特别适用于数据并行计算。

本节将重点介绍如何在 Rust 中使用这些并发 API，特别是如何确保线程安全和共享资源的安全访问。

## 二、标准库中的并发 API

Rust 的标准库提供了一些用于并发编程的基本功能，如线程创建、原子引用计数等。以下是一个简单的示例，展示了如何使用标准库来创建多个线程并共享数据：

```rust
use std::sync::{Arc, Mutex};  // 导入 Arc 和 Mutex 类型，用于实现线程安全的共享数据
use std::thread;  // 导入 thread 模块，用于创建和管理线程

fn main() {
    // 创建一个 `Mutex`，初始值为 0，用于保护共享数据。将其放在 `Arc` 中，以便可以安全地在线程间共享。
    // `Arc` 是一个线程安全的引用计数智能指针，允许多线程共享所有权。
    let counter = Arc::new(Mutex::new(0));  // Arc<Mutex<i32>> 用于多线程间共享对 `counter` 的所有权

    let mut handles = vec![];  // 用于存储所有线程句柄的向量，后面需要等待这些线程完成

    // 启动 10 个线程，每个线程都将 `counter` 增加 1
    for _ in 0..10 {
        // 克隆 `Arc`，为了将 `counter` 的引用传递到每个线程中
        let counter = Arc::clone(&counter);
        
        // 创建并启动一个线程
        let handle = thread::spawn(move || {
            // 每个线程中，我们通过 `lock()` 获取 `Mutex` 的锁。`unwrap()` 会在失败时 panic
            let mut num = counter.lock().unwrap();
            // 解除锁并访问内部数据，增加其值
            *num += 1;  // 通过解引用（`*num`）修改共享的 `counter` 值
        });

        // 将当前线程的句柄存储到 `handles` 中
        handles.push(handle);
    }

    // 等待所有线程完成
    for handle in handles {
        handle.join().unwrap();  // `join()` 会阻塞当前线程，直到每个子线程完成
    }

    // 输出最终结果，锁住 `counter`，并获取其值，输出结果
    // 由于 `Mutex` 被锁住，所以只能一个线程访问 `counter`，确保线程安全
    println!("Result: {}", *counter.lock().unwrap());  // 解锁并打印 `counter` 最终的值
}

```

在这个例子中，`Arc`（原子引用计数）允许在多个线程间安全共享数据，而 `Mutex` 提供了对共享数据的互斥访问。通过这种方式，我们能够在多个线程间共享一个计数器，并确保每个线程在修改计数器时不会发生竞争条件。

## 三、线程与原子引用计数

Rust 中的 `Arc` 是一个线程安全的引用计数智能指针，它允许在多个线程之间共享数据。`Mutex` 也同样是线程安全的，它保证同一时刻只有一个线程可以访问被保护的数据。让我们看一个关于 `Arc` 的简单示例：

```rust
use std::sync::Arc;  // 导入 Arc 类型，用于在多线程间安全共享数据
use std::thread;  // 导入 thread 模块，用于创建和管理线程

fn main() {
    // 创建一个 `Arc`，并用它来共享值 5。`Arc` 使得数据可以在线程间共享所有权。
    let value = Arc::new(5);  // `Arc` 是一个原子引用计数智能指针，允许多个线程共享对数据的所有权

    let mut handles = vec![];  // 用于存储所有线程的句柄，以便后面等待它们完成

    // 启动 10 个线程，每个线程打印 `value` 的值
    for _ in 0..10 {
        // 克隆 `Arc`，传递给每个新线程。`Arc::clone` 增加引用计数，允许共享数据
        let value = Arc::clone(&value);  // 通过 `Arc::clone` 克隆一个新的引用，传递给每个线程
        
        // 创建并启动线程
        let handle = thread::spawn(move || {
            // 每个线程打印共享的数据 `value`
            // `move` 关键字将 `value` 的所有权转移到闭包中，确保 `value` 在闭包中是有效的
            println!("{}", value);  // 打印共享的 `value`，所有线程都会打印值 `5`
        });

        handles.push(handle);  // 将线程句柄添加到 `handles` 向量中，以便稍后调用 `join()` 等待线程完成
    }

    // 等待所有线程完成
    for handle in handles {
        handle.join().unwrap();  // 调用 `join()` 等待线程结束。`unwrap()` 会在遇到错误时 panic
    }
}

```

在这个例子中，`Arc` 用于跨线程共享 `value`，并且确保该数据在所有线程完成之前不会被销毁。每个线程都通过克隆 `Arc` 来访问共享数据，并且 `Arc` 确保数据的引用计数正确管理。

## 四、线程安全与 `Send` 和 `Sync` 标记

Rust 使用 `Send` 和 `Sync` trait 来确保数据在多线程环境下的安全性。`Send` trait 表示一个类型可以安全地传递到另一个线程，而 `Sync` trait 则表示一个类型可以在多个线程间共享。标准库中的并发 API 都依赖这些标记来确保类型的正确性和安全性。

例如，`Arc<T>` 可以在多个线程之间安全地传递，前提是 `T` 类型实现了 `Send` 和 `Sync`。如果类型 `T` 不实现这些 trait，则无法将 `Arc<T>` 从一个线程传递到另一个线程。

```rust
use std::sync::Arc;
use std::thread;

fn main() {
    let x = Arc::new(5);

    // 此处，5 是可发送的，并且 `Arc` 也可以安全地传递
    let y = Arc::clone(&x);

    let handle = thread::spawn(move || {
        println!("Value: {}", y);  // 使用传递给新线程的数据
    });

    handle.join().unwrap();
}
```

在这个例子中，`x` 被包装在 `Arc` 中，可以在多个线程之间共享。Rust 保证了只要 `T` 类型满足 `Send` 和 `Sync` 的要求，线程间的共享就是安全的。

### 手动实现 `Send` 和 `Sync`

虽然 Rust 会自动为符合条件的类型实现 `Send` 和 `Sync`，但你也可以手动为自定义类型实现这两个 trait。下面是一个手动实现 `Send` 和 `Sync` 的例子：

```rust
use std::sync::{Arc, Mutex};
use std::thread;

// 自定义类型
struct MyStruct {
    data: i32,
}

// 手动实现 `Send` trait
unsafe impl Send for MyStruct {}

// 手动实现 `Sync` trait
unsafe impl Sync for MyStruct {}

fn main() {
    let my_data = Arc::new(Mutex::new(MyStruct { data: 42 }));

    let mut handles = vec![];

    for _ in 0..10 {
        let my_data = Arc::clone(&my_data); // 克隆 Arc 以便多个线程共享

        let handle = thread::spawn(move || {
            let mut my_data = my_data.lock().unwrap(); // 获取锁
            my_data.data += 1; // 增加共享数据
            println!("Data in thread: {}", my_data.data);
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap(); // 等待所有线程完成
    }

    // 最终值
    println!("Final result: {}", my_data.lock().unwrap().data);
}
```

在这个例子中，我们手动为 `MyStruct` 类型实现了 `Send` 和 `Sync`。这允许我们安全地在多个线程间传递和共享 `MyStruct` 的数据。需要注意的是，我们使用了 `unsafe` 来表示我们手动保证这些实现是安全的。

## 总结

* **`Send`** 允许类型的实例安全地从一个线程传递到另一个线程。
* **`Sync`** 允许类型在多个线程之间共享引用，确保线程间访问数据时的安全性。
* 在 Rust 中，`Arc<T>` 和 `Mutex<T>` 是线程安全的，前提是它们的类型 `T` 实现了 `Send` 和 `Sync`。

这些特性帮助 Rust 在并发编程中避免常见的错误，如数据竞争和线程安全问题，保证了程序在多线程环境中的安全性和稳定性。

## 五、`Mutex` 和 `RwLock`：互斥锁与读写锁

`Mutex` 和 `RwLock` 是 Rust 提供的两种常用的同步原语。`Mutex` 提供了排他访问，而 `RwLock` 允许多个线程同时读取，但在写操作时会独占访问权限。通过这两种机制，我们可以更灵活地控制数据的访问权限。

例如，`RwLock` 允许多个线程同时读取共享数据，但当一个线程需要修改数据时，它会获得写锁，阻止其他线程读取或修改数据。

## 六、总结与安全 API

Rust 提供的并发库通过将潜在的非安全实现封装在安全的 API 中，确保了并发编程的安全性。通过使用 `Arc`、`Mutex` 和 `RwLock` 等工具，开发者可以轻松地在多线程环境中共享和保护数据。同时，Rust 的 `Send` 和 `Sync` 特性确保了类型在并发环境中的安全性，避免了因共享数据而导致的错误。

在并发编程中，理解和使用 Rust 提供的这些安全 API 不仅有助于简化编程流程，还能有效地避免常见的并发错误，如数据竞争、死锁等。

