//! Snapshot undo/redo with the same semantics as winshot's `useUndoRedo`:
//! at most 50 past states, and any new change clears the redo stack.

#[derive(Debug, Clone)]
pub struct History<T> {
    past: Vec<T>,
    present: T,
    future: Vec<T>,
    max: usize,
}

impl<T: Clone + PartialEq> History<T> {
    pub const DEFAULT_MAX: usize = 50;

    pub fn new(initial: T) -> Self {
        Self { past: Vec::new(), present: initial, future: Vec::new(), max: Self::DEFAULT_MAX }
    }

    pub fn present(&self) -> &T {
        &self.present
    }

    /// Record a new state. A state equal to the current one is ignored, so a
    /// click without a drag does not create an undo step.
    pub fn push(&mut self, next: T) {
        if next == self.present {
            return;
        }
        self.past.push(std::mem::replace(&mut self.present, next));
        if self.past.len() > self.max {
            self.past.remove(0);
        }
        self.future.clear();
    }

    pub fn undo(&mut self) -> Option<&T> {
        let prev = self.past.pop()?;
        self.future.insert(0, std::mem::replace(&mut self.present, prev));
        Some(&self.present)
    }

    pub fn redo(&mut self) -> Option<&T> {
        if self.future.is_empty() {
            return None;
        }
        let next = self.future.remove(0);
        self.past.push(std::mem::replace(&mut self.present, next));
        Some(&self.present)
    }

    /// Start over from `state` with no undo or redo steps.
    pub fn reset(&mut self, state: T) {
        self.past.clear();
        self.future.clear();
        self.present = state;
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_redo_round_trip() {
        let mut h = History::new(0);
        h.push(1);
        h.push(2);
        assert_eq!(h.undo(), Some(&1));
        assert_eq!(h.undo(), Some(&0));
        assert_eq!(h.undo(), None);
        assert_eq!(h.redo(), Some(&1));
        assert_eq!(h.redo(), Some(&2));
        assert_eq!(h.redo(), None);
    }

    #[test]
    fn new_change_clears_redo() {
        let mut h = History::new(0);
        h.push(1);
        h.undo();
        h.push(5);
        assert!(!h.can_redo());
        assert_eq!(*h.present(), 5);
    }

    #[test]
    fn history_is_capped() {
        let mut h = History::new(0);
        for i in 1..=60 {
            h.push(i);
        }
        let mut steps = 0;
        while h.undo().is_some() {
            steps += 1;
        }
        assert_eq!(steps, 50);
        assert_eq!(*h.present(), 10);
    }

    #[test]
    fn identical_state_is_not_recorded() {
        let mut h = History::new(3);
        h.push(3);
        assert!(!h.can_undo());
    }
}
