//! Undo and redo in the map editor, per action: each stroke, each change in the inspector,
//! each move keeps the whole state of the tiles before and after. Undo sends back the
//! "before"; redo, the "after".

use uo_core::MapTile;

/// One editor action: the tiles as they were and as they became, in the same order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Action {
    pub before: Vec<MapTile>,
    pub after: Vec<MapTile>,
}

#[derive(Debug, Default)]
pub struct History {
    done: Vec<Action>,
    undone: Vec<Action>,
}

impl History {
    /// How many actions are kept.
    const KEEP: usize = 200;

    /// A new action: what was undone can no longer come back.
    pub fn push(&mut self, action: Action) {
        self.undone.clear();
        self.done.push(action);
        if self.done.len() > Self::KEEP {
            self.done.remove(0);
        }
    }

    /// The action to undo (apply its `before`); it can then be redone.
    pub fn undo(&mut self) -> Option<Action> {
        let action = self.done.pop()?;
        self.undone.push(action.clone());
        Some(action)
    }

    /// The action to redo (apply its `after`).
    pub fn redo(&mut self) -> Option<Action> {
        let action = self.undone.pop()?;
        self.done.push(action.clone());
        Some(action)
    }

    pub fn can_undo(&self) -> bool {
        !self.done.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(z: i8) -> MapTile {
        MapTile {
            land_z: Some(z),
            ..MapTile::fresh(1, 1)
        }
    }

    fn action(from: i8, to: i8) -> Action {
        Action {
            before: vec![tile(from)],
            after: vec![tile(to)],
        }
    }

    #[test]
    fn undo_and_redo_walk_the_actions_and_a_new_one_drops_the_redo() {
        let mut history = History::default();
        assert!(!history.can_undo() && history.undo().is_none());
        history.push(action(0, 5));
        history.push(action(5, 10));
        assert_eq!(history.undo(), Some(action(5, 10)));
        assert_eq!(history.undo(), Some(action(0, 5)));
        assert!(!history.can_undo() && history.can_redo());
        assert_eq!(history.redo(), Some(action(0, 5)));
        history.push(action(5, 7));
        assert!(
            !history.can_redo(),
            "the new action drops what could be redone"
        );
        assert_eq!(history.undo(), Some(action(5, 7)));
    }

    #[test]
    fn only_the_last_actions_are_kept() {
        let mut history = History::default();
        for z in 0..250 {
            history.push(action(z as i8, (z as i8).wrapping_add(1)));
        }
        let mut count = 0;
        while history.undo().is_some() {
            count += 1;
        }
        assert_eq!(count, History::KEEP);
    }
}
