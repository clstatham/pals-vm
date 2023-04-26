use std::{
    fmt::{Debug, Display},
    sync::atomic::AtomicUsize,
    time::Duration,
};

use async_timer::Interval;
use derive_more::*;
use tokio::{sync::watch, task::JoinHandle};

use crate::HEARTBEAT;

#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    From,
    Into,
    Constructor,
    Not,
    BitAnd,
    BitOr,
    BitXor,
)]
pub struct Bit(bool);

impl Bit {
    pub const HI: Self = Self(true);
    pub const LO: Self = Self(false);

    pub const fn as_u8(self) -> u8 {
        if self.0 {
            1
        } else {
            0
        }
    }
}

impl Debug for Bit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0 {
            write!(f, "1")
        } else {
            write!(f, "0")
        }
    }
}

impl Display for Bit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

static BIT_IDS: AtomicUsize = AtomicUsize::new(0);

/// Async Bit
#[derive(Debug)]
pub struct ABit {
    id: usize,
    pub behavior: ABitBehavior,
    set_rx: watch::Receiver<Bit>,
    get_tx: watch::Sender<Bit>,
    _get_rx: watch::Receiver<Bit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ABitBehavior {
    Normal { value: Bit },
    Clock { half_period: Duration },
    AlwaysHi,
    AlwaysLo,
}

impl ABit {
    pub fn new(behavior: ABitBehavior, set_rx: watch::Receiver<Bit>) -> Self {
        let initial = if let ABitBehavior::Normal { value } = behavior {
            value
        } else {
            Bit::LO
        };
        let (get_tx, _get_rx) = watch::channel(initial);
        Self {
            id: BIT_IDS.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            behavior,
            set_rx,
            get_tx,
            _get_rx,
        }
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn subscribe(&self) -> watch::Receiver<Bit> {
        self.get_tx.subscribe()
    }

    fn spawn_always_hi(self) -> JoinHandle<()> {
        let mut interval = Interval::platform_new(HEARTBEAT);
        tokio::spawn(async move {
            loop {
                match self.get_tx.send(Bit::HI) {
                    Ok(_) => {}
                    Err(_) => {
                        println!("PBit {:?} send() Closed", self.id);
                        return;
                    }
                }

                interval.wait().await;
                tokio::task::yield_now().await;
            }
        })
    }

    fn spawn_always_lo(self) -> JoinHandle<()> {
        let mut interval = Interval::platform_new(HEARTBEAT);
        tokio::spawn(async move {
            loop {
                match self.get_tx.send(Bit::LO) {
                    Ok(_) => {}
                    Err(_) => {
                        println!("PBit {:?} send() Closed", self.id);
                        return;
                    }
                }

                interval.wait().await;
                tokio::task::yield_now().await;
            }
        })
    }

    fn spawn_clock(self) -> JoinHandle<()> {
        if let ABitBehavior::Clock { half_period } = self.behavior {
            let mut interval = Interval::platform_new(half_period);
            tokio::spawn(async move {
                loop {
                    match self.get_tx.send(Bit::HI) {
                        Ok(_) => {}
                        Err(_) => {
                            println!("PBit {:?} send() Closed", self.id);
                            return;
                        }
                    }

                    interval.wait().await;

                    match self.get_tx.send(Bit::LO) {
                        Ok(_) => {}
                        Err(_) => {
                            println!("PBit {:?} send() Closed", self.id);
                            return;
                        }
                    }

                    interval.wait().await;
                }
            })
        } else {
            unreachable!()
        }
    }

    pub fn spawn_eager(self) -> JoinHandle<()> {
        match self.behavior {
            ABitBehavior::AlwaysHi => self.spawn_always_hi(),
            ABitBehavior::AlwaysLo => self.spawn_always_lo(),
            ABitBehavior::Clock { .. } => self.spawn_clock(),
            ABitBehavior::Normal { .. } => self.spawn_normal(),
        }
    }

    fn spawn_normal(mut self) -> JoinHandle<()> {
        let mut interval = Interval::platform_new(HEARTBEAT);
        if let ABitBehavior::Normal { .. } = self.behavior {
            tokio::spawn(async move {
                loop {
                    let new_bit = *self.set_rx.borrow_and_update();
                    self.get_tx.send(new_bit).ok();
                    interval.wait().await;
                    tokio::task::yield_now().await;
                }
            })
        } else {
            unreachable!()
        }
    }
}

#[derive(Debug)]
pub enum SpawnResult {
    Ok,
    NotConnected,
    NotApplicable,
}

#[derive(Debug)]
pub enum UpdateResult {
    Modified,
    Ok,
    NotConnected,
    NotApplicable,
    RecvError,
}
