//! What is picked, in a listing whose rows are not all loaded.
//!
//! Select-all cannot mean "every key on screen" when only a window is held, so it is recorded as
//! the filter itself and resolved to keys once, at the moment a command needs them.

use std::collections::HashSet;

/// Either the rows that were clicked, or the whole filtered listing however long it is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    picked: Vec<String>,
    held: HashSet<String>,
    /// The filtered listing entire, rather than the keys that happen to be loaded.
    everything: bool,
    /// Where a shift-click extends from, as an index into the whole listing.
    anchor: Option<usize>,
    /// Where the arrows are, as an index into the whole listing.
    cursor: Option<usize>,
}

impl Selection {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the whole filtered listing is picked, loaded or not.
    pub fn is_everything(&self) -> bool {
        self.everything
    }

    /// Whether anything at all is picked, which is what raises the selection bar.
    pub fn is_active(&self) -> bool {
        self.everything || !self.picked.is_empty()
    }

    /// How many rows this stands for. `total` is the whole filtered listing's length.
    pub fn count(&self, total: usize) -> usize {
        match self.everything {
            true => total,
            false => self.picked.len(),
        }
    }

    pub fn contains(&self, id: &str) -> bool {
        self.everything || self.held.contains(id)
    }

    /// The keys that were actually clicked; empty under select-all, which has to be resolved.
    pub fn picked(&self) -> &[String] {
        &self.picked
    }

    pub fn cursor(&self) -> Option<usize> {
        self.cursor
    }

    pub fn set_cursor(&mut self, index: Option<usize>) {
        self.cursor = index;
    }

    pub fn clear(&mut self) {
        let cursor = self.cursor;
        *self = Self::default();
        self.cursor = cursor;
    }

    /// Ctrl-click: add or drop one row, and make it where a later shift-click reaches from.
    pub fn toggle(&mut self, id: &str, index: usize) {
        // Turning select-all into a set of keys would need the whole listing; start over instead.
        if self.everything {
            self.everything = false;
            self.picked.clear();
            self.held.clear();
        }
        if self.held.remove(id) {
            self.picked.retain(|held| held != id);
        } else {
            self.held.insert(id.to_string());
            self.picked.push(id.to_string());
            self.anchor = Some(index);
        }
        self.cursor = Some(index);
    }

    /// Shift-click: everything between the last pick and `index`, as far as the window holds it.
    ///
    /// `rows` answers what sits at an index; a row outside the loaded window has no key to add,
    /// so a range dragged across one covers what is on screen rather than silently picking rows
    /// nobody has seen.
    pub fn extend_to(&mut self, index: usize, rows: impl Fn(usize) -> Option<String>) {
        let anchor = self.anchor.unwrap_or(index);
        let (first, last) = (anchor.min(index), anchor.max(index));
        if self.everything {
            self.everything = false;
            self.picked.clear();
            self.held.clear();
        }
        for slot in first..=last {
            let Some(id) = rows(slot) else { continue };
            if self.held.insert(id.clone()) {
                self.picked.push(id);
            }
        }
        self.cursor = Some(index);
    }

    /// Ctrl-A: the whole filtered listing, not the window. Nothing is fetched to say so.
    pub fn select_everything(&mut self) {
        self.picked.clear();
        self.held.clear();
        self.everything = true;
    }

    /// Drop the keys a refetch no longer holds, so a re-sort cannot leave a count over nothing.
    ///
    /// Select-all is scoped to the filter rather than to keys, so a refetch leaves it alone.
    pub fn retain_loaded(&mut self, loaded: &HashSet<String>, complete: bool) {
        if self.everything || !complete {
            return;
        }
        self.picked.retain(|id| loaded.contains(id));
        self.held.retain(|id| loaded.contains(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(index: usize) -> Option<String> {
        (index < 10).then(|| format!("k{index}"))
    }

    #[test]
    fn nothing_picked_raises_no_bar() {
        let selection = Selection::new();
        assert!(!selection.is_active());
        assert_eq!(selection.count(5000), 0);
    }

    #[test]
    fn select_all_stands_for_the_whole_listing_without_holding_a_key() {
        let mut selection = Selection::new();
        selection.select_everything();
        assert!(selection.is_everything());
        assert_eq!(selection.count(40_000), 40_000);
        assert!(selection.picked().is_empty());
        assert!(selection.contains("a track nobody has loaded"));
    }

    #[test]
    fn a_click_after_select_all_starts_a_plain_selection() {
        let mut selection = Selection::new();
        selection.select_everything();
        selection.toggle("k1", 1);
        assert!(!selection.is_everything());
        assert_eq!(selection.picked(), ["k1"]);
        assert_eq!(selection.count(40_000), 1);
    }

    #[test]
    fn a_range_covers_what_the_window_holds() {
        let mut selection = Selection::new();
        selection.toggle("k2", 2);
        selection.extend_to(15, rows);
        assert_eq!(selection.count(100), 8);
        assert!(selection.contains("k9"));
    }

    #[test]
    fn a_refetch_drops_keys_that_are_gone_but_leaves_select_all_alone() {
        let mut selection = Selection::new();
        selection.toggle("k1", 1);
        selection.toggle("k2", 2);
        let loaded = HashSet::from(["k2".to_string()]);
        selection.retain_loaded(&loaded, true);
        assert_eq!(selection.picked(), ["k2"]);

        let mut everything = Selection::new();
        everything.select_everything();
        everything.retain_loaded(&HashSet::new(), true);
        assert!(everything.is_everything());
    }

    #[test]
    fn a_partial_window_never_drops_a_pick_that_is_simply_not_loaded() {
        let mut selection = Selection::new();
        selection.toggle("k1", 1);
        selection.retain_loaded(&HashSet::new(), false);
        assert_eq!(selection.picked(), ["k1"]);
    }

    #[test]
    fn toggling_the_same_row_twice_leaves_nothing_picked() {
        let mut selection = Selection::new();
        selection.toggle("k1", 1);
        selection.toggle("k1", 1);
        assert!(!selection.is_active());
    }
}
