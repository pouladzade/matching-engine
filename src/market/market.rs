use anyhow::{Context, Result};
use crossbeam::channel;
use std::sync::{Arc, RwLock};
use std::thread;

use crate::models::order::Order;
use crate::models::trade::Trade;
use crate::order_book::order_book::OrderBook;
use crate::order_book::order_book::OrderBookTrait;

type Task = Box<dyn FnOnce(&mut OrderBook) + Send + 'static>;

#[derive(Debug, Clone)]
pub struct Market {
    task_sender: channel::Sender<Task>,
    order_book: Arc<RwLock<OrderBook>>, // RwLock allows concurrent reads
}

impl Market {
    pub fn new(pool_size: usize) -> Self {
        let (task_sender, task_receiver) = channel::unbounded();
        let order_book = Arc::new(RwLock::new(OrderBook::new())); // Use RwLock

        for _ in 0..pool_size {
            let task_receiver: channel::Receiver<Task> = task_receiver.clone();
            let order_book = Arc::clone(&order_book);

            thread::spawn(move || {
                while let Ok(task) = task_receiver.recv() {
                    if let Ok(mut order_book) = order_book.write() {
                        task(&mut order_book);
                    } else {
                        panic!("Failed to acquire write lock on order_book");
                    }
                }
            });
        }

        Market {
            task_sender,
            order_book,
        }
    }

    fn submit_task(&self, task: Task) -> Result<()> {
        self.task_sender
            .send(task)
            .map_err(|e| anyhow::anyhow!("Failed to send task to worker thread: {}", e))
    }

    pub fn add_order(&self, order: Order) -> Result<Vec<Trade>> {
        let (sender, receiver) = std::sync::mpsc::channel();

        self.submit_task(Box::new(move |order_book: &mut OrderBook| {
            let trades = order_book.add_order(order);
            let _ = sender.send(trades);
        }))?;

        receiver
            .recv()
            .context("Failed to receive order execution result")
    }

    pub fn get_order_by_id(&self, order_id: u64) -> Result<Option<Order>> {
        let order_book = self
            .order_book
            .read()
            .map_err(|e| anyhow::anyhow!("Failed to send task to worker thread: {}", e))?;
        Ok(order_book.get_order_by_id(order_id)) // No need for a task
    }

    pub fn cancel_order(&self, order_id: u64) -> Result<bool> {
        let (sender, receiver) = std::sync::mpsc::channel();

        self.submit_task(Box::new(move |order_book: &mut OrderBook| {
            let canceled = order_book.cancel_order(order_id);
            let _ = sender.send(canceled);
        }))?;

        receiver
            .recv()
            .context("Failed to receive order cancellation result")
    }

    pub fn cancel_all_orders(&self) -> Result<bool> {
        let (sender, receiver) = std::sync::mpsc::channel();

        self.submit_task(Box::new(move |order_book: &mut OrderBook| {
            let canceled = order_book.cancel_all_orders();
            let _ = sender.send(canceled);
        }))?;

        receiver
            .recv()
            .context("Failed to receive all orders cancellation result")
    }
}
