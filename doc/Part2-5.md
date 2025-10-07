# KAIST CS492 - 并发编程 · 课程速读（Part2.5 Rust中的安全API - Crossbeam API）

Rust 中的并发库 **Crossbeam**是一个非常强大且广泛使用的库，特别是在处理复杂并发任务时。`Crossbeam` 提供了比标准库更强大的工具和抽象，简化了并发编程的许多方面。在这一节中，我们将讨论 `Crossbeam` 的几个关键功能，如 **Scoped Threads**、**Cache Padded** 和 **Channels**。

## 一、`Scoped Thread`（作用域线程）

Rust 标准库中的线程模型要求线程的生命周期严格受限，不能随意共享栈上变量。然而，**`Crossbeam` 的 `scope`** 提供了一种灵活的方式，允许我们在不违反 Rust 的借用规则的前提下，安全地共享数据。

在 `Crossbeam` 中，**`scope`** 允许线程共享栈上变量，并确保在退出作用域之前，所有线程都已完成。这种机制比标准库中的 `thread::spawn` 更加灵活，因为它允许线程在不同的作用域内共享数据，并在作用域结束时确保数据的安全。

### 示例代码：

```rust
use crossbeam::scope;

fn main() {
    let greeting = String::from("Hello, world!");

    scope(|s| {
        // 在两个线程中共享 greeting 数据
        s.spawn(|_| {
            println!("{}", greeting);
        });
        s.spawn(|_| {
            println!("{}", greeting);
        });
    }).unwrap(); // 确保所有线程在作用域结束前执行完毕
}
```

### 关键点：

* **作用域线程**：`scope` 可以在多个线程之间安全地共享栈上的变量。所有线程必须在作用域结束前完成工作，确保数据不会在线程使用时被丢弃或释放。
* **线程安全性**：`Crossbeam` 确保线程在共享数据时，遵循 Rust 的所有权和借用规则，避免了因数据生命周期不一致而引发的错误。

## 二、`Cache Padded`（缓存填充）

在并发编程中，**假共享（False Sharing）** 是一种性能问题。当多个线程并行访问不同的数据，但这些数据位于同一个缓存行时，可能会引发性能下降。为了避免这种情况，**`Crossbeam`** 提供了 **`Cache Padded`**，它会在数据周围填充无效数据，确保每个数据项位于不同的缓存行中，从而避免线程之间的缓存干扰。

### 示例代码：

```rust
use crossbeam::utils::CachePadded;

struct Data {
    a: CachePadded<i64>,
    b: CachePadded<i64>,
}

fn main() {
    let data = Data {
        a: CachePadded::new(0),
        b: CachePadded::new(0),
    };

    // 数据 a 和 b 将不会共享缓存行，避免假共享
}
```

### 关键点：

* **假共享**：多个线程并发访问不同的数据项时，如果这些数据位于同一缓存行，会导致缓存一致性问题，影响性能。
* **`CachePadded`**：`Crossbeam` 的 `CachePadded` 通过在数据项周围填充无效数据，确保它们位于不同的缓存行，避免了假共享问题，提升了并发性能。

## 三、`Channel`（通道）

**`Crossbeam` 的通道**提供了多种并发通信方式。它支持 **多生产者-单消费者（MPSC）** 和 **多生产者-多消费者（MPMC）** 模式。这使得 `Crossbeam` 的通道在并发任务中更加灵活和高效，能够支持复杂的线程间通信。

`Crossbeam` 提供的通道有 **有界** 和 **无界** 两种版本。**无界通道**允许生产者发送任意数量的数据，直到内存不足为止，而 **有界通道**在达到最大容量时会返回错误，避免内存溢出。

### 示例代码：

```rust
use crossbeam::channel;

fn main() {
    let (sender, receiver) = channel::unbounded();

    // 创建多个生产者
    std::thread::spawn(move || {
        sender.send(1).unwrap();
    });
    std::thread::spawn(move || {
        sender.send(2).unwrap();
    });

    // 消费者接收数据
    for _ in 0..2 {
        let msg = receiver.recv().unwrap();
        println!("Received: {}", msg);
    }
}
```

### 关键点：

* **MPSC/MPMC**：`Crossbeam` 支持多生产者和多消费者的通道，可以让多个线程同时发送数据到通道，并由一个或多个消费者处理这些数据。
* **`unbounded()`** 和 **`bounded()`**：无界通道不限制消息的数量，直到系统内存耗尽；有界通道在达到预定的容量后返回错误，确保系统稳定运行。

## 四、总结

在这一节中，我们介绍了 `Crossbeam` 库中的三个主要并发功能：

1. **Scoped Threads**：提供了灵活的线程管理，允许在不同的线程间共享栈上数据，并保证线程在作用域结束前完成，避免数据在使用时被释放。
2. **Cache Padded**：通过缓存填充避免了假共享问题，从而提高了并发性能。
3. **Channel**：提供了多种类型的并发通信模式，支持多生产者和多消费者，可以在复杂的并发场景中高效地传递数据。

`Crossbeam` 是一个功能强大且灵活的并发库，它通过提供更高层次的抽象，简化了并发编程中的许多细节。它让开发者能够轻松地处理多线程和并发任务，并确保高效且安全的并行执行。
