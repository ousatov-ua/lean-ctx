//! Bounded stores, queues, and circuit breakers for kernel HA.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// A first-in, first-out queue with a fixed upper bound.
#[derive(Debug, Clone)]
pub struct BoundedQueue<T> {
    items: VecDeque<T>,
    max_size: usize,
}

impl<T> BoundedQueue<T> {
    /// Creates an empty queue that retains at most `max_size` items.
    pub fn new(max_size: usize) -> Self {
        Self {
            items: VecDeque::with_capacity(max_size),
            max_size,
        }
    }

    /// Appends an item and returns the oldest item when capacity is exceeded.
    pub fn push(&mut self, item: T) -> Option<T> {
        if self.max_size == 0 {
            return Some(item);
        }

        let evicted = if self.is_full() {
            self.items.pop_front()
        } else {
            None
        };
        self.items.push_back(item);
        evicted
    }

    /// Returns the number of retained items.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns whether the queue contains no items.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Returns whether the queue has reached its configured capacity.
    pub fn is_full(&self) -> bool {
        self.items.len() >= self.max_size
    }

    /// Iterates over retained items from oldest to newest.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items.iter()
    }

    /// Removes and returns up to `n` oldest items.
    pub fn drain_oldest(&mut self, n: usize) -> Vec<T> {
        let count = n.min(self.items.len());
        self.items.drain(..count).collect()
    }

    /// Removes all retained items.
    pub fn clear(&mut self) {
        self.items.clear();
    }
}

/// Operational state of a circuit breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Requests are allowed normally.
    Closed,
    /// Requests are rejected until the cooldown elapses.
    Open,
    /// Requests are allowed while provider recovery is evaluated.
    HalfOpen,
}

/// Failure isolation state for a fallible dependency.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    failure_threshold: u32,
    recovery_threshold: u32,
    last_state_change: Instant,
    cooldown: Duration,
}

impl CircuitBreaker {
    /// Creates a circuit breaker with caller-defined thresholds and cooldown.
    pub fn new(failure_threshold: u32, recovery_threshold: u32, cooldown: Duration) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            success_count: 0,
            failure_threshold,
            recovery_threshold,
            last_state_change: Instant::now(),
            cooldown,
        }
    }

    /// Creates the standard kernel circuit breaker.
    pub fn default_breaker() -> Self {
        Self::new(3, 2, Duration::from_secs(30))
    }

    /// Returns the current circuit state.
    pub fn state(&self) -> CircuitState {
        self.state
    }

    /// Records a successful dependency call.
    pub fn record_success(&mut self) {
        match self.state {
            CircuitState::Closed => {
                self.failure_count = 0;
            }
            CircuitState::HalfOpen => {
                self.success_count = self.success_count.saturating_add(1);
                if self.success_count >= self.recovery_threshold {
                    self.transition_to(CircuitState::Closed);
                }
            }
            CircuitState::Open => {}
        }
    }

    /// Records a failed dependency call.
    pub fn record_failure(&mut self) {
        match self.state {
            CircuitState::Closed => {
                self.failure_count = self.failure_count.saturating_add(1);
                if self.failure_count >= self.failure_threshold {
                    self.transition_to(CircuitState::Open);
                }
            }
            CircuitState::HalfOpen => self.transition_to(CircuitState::Open),
            CircuitState::Open => {}
        }
    }

    /// Returns whether a dependency call should proceed.
    ///
    /// **Side effect**: if the breaker is `Open` and the cooldown has
    /// elapsed, this transitions the state to `HalfOpen`.
    pub fn should_allow(&mut self) -> bool {
        match self.state {
            CircuitState::Closed | CircuitState::HalfOpen => true,
            CircuitState::Open => {
                if self.last_state_change.elapsed() >= self.cooldown {
                    self.transition_to(CircuitState::HalfOpen);
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Forces the circuit into its initial closed state.
    pub fn reset(&mut self) {
        self.transition_to(CircuitState::Closed);
    }

    fn transition_to(&mut self, state: CircuitState) {
        self.state = state;
        self.failure_count = 0;
        self.success_count = 0;
        self.last_state_change = Instant::now();
    }
}

/// Circuit breaker and lifetime call statistics for one provider.
#[derive(Debug, Clone)]
pub struct ProviderCircuit {
    pub provider_id: String,
    pub breaker: CircuitBreaker,
    pub total_calls: u64,
    pub total_failures: u64,
}

impl ProviderCircuit {
    /// Creates provider state using the standard kernel breaker settings.
    pub fn new(provider_id: String) -> Self {
        Self {
            provider_id,
            breaker: CircuitBreaker::default_breaker(),
            total_calls: 0,
            total_failures: 0,
        }
    }

    /// Returns whether the provider circuit currently permits a call.
    pub fn is_available(&mut self) -> bool {
        self.breaker.should_allow()
    }

    /// Records a completed provider call and updates circuit state.
    pub fn record_outcome(&mut self, success: bool) {
        self.total_calls = self.total_calls.saturating_add(1);
        if success {
            self.breaker.record_success();
        } else {
            self.total_failures = self.total_failures.saturating_add(1);
            self.breaker.record_failure();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BoundedQueue, CircuitBreaker, CircuitState, ProviderCircuit};
    use std::time::Duration;

    #[test]
    fn bounded_queue_evicts_oldest() {
        let mut queue = BoundedQueue::new(2);

        assert_eq!(queue.push(1), None);
        assert_eq!(queue.push(2), None);
        assert_eq!(queue.push(3), Some(1));
        assert_eq!(queue.iter().copied().collect::<Vec<i32>>(), vec![2, 3]);
        assert!(queue.is_full());
    }

    #[test]
    fn bounded_queue_drain() {
        let mut queue = BoundedQueue::new(4);
        queue.push("first");
        queue.push("second");
        queue.push("third");

        assert_eq!(queue.drain_oldest(2), vec!["first", "second"]);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.drain_oldest(5), vec!["third"]);
        assert!(queue.is_empty());
    }

    #[test]
    fn bounded_queue_with_zero_capacity_rejects_items() {
        let mut queue = BoundedQueue::new(0);

        assert_eq!(queue.push(7), Some(7));
        assert!(queue.is_empty());
        assert!(queue.is_full());
    }

    #[test]
    fn circuit_breaker_opens_after_threshold() {
        let mut breaker = CircuitBreaker::new(3, 2, Duration::from_secs(30));

        breaker.record_failure();
        breaker.record_failure();
        assert_eq!(breaker.state(), CircuitState::Closed);
        breaker.record_failure();

        assert_eq!(breaker.state(), CircuitState::Open);
        assert!(!breaker.should_allow());
    }

    #[test]
    fn circuit_breaker_recovers_via_half_open() {
        let mut breaker = CircuitBreaker::new(1, 1, Duration::ZERO);
        breaker.record_failure();

        assert!(breaker.should_allow());
        assert_eq!(breaker.state(), CircuitState::HalfOpen);
        breaker.record_success();

        assert_eq!(breaker.state(), CircuitState::Closed);
        assert!(breaker.should_allow());
    }

    #[test]
    fn provider_circuit_tracks_stats() {
        let mut circuit = ProviderCircuit::new("filesystem".to_string());

        circuit.record_outcome(true);
        circuit.record_outcome(false);
        circuit.record_outcome(false);

        assert_eq!(circuit.total_calls, 3);
        assert_eq!(circuit.total_failures, 2);
        assert_eq!(circuit.breaker.state(), CircuitState::Closed);
    }
}
