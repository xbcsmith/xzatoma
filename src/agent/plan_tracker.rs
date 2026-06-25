//! Plan entry detection and status tracking for ACP sessions.
//!
//! Parses streamed assistant output for numbered-list plan items and tracks
//! their execution status so that `AcpSessionObserver` can emit live
//! `SessionUpdate::Plan` notifications to Zed.
//!
//! # Examples
//!
//! ```
//! use xzatoma::agent::plan_tracker::PlanTracker;
//!
//! let mut tracker = PlanTracker::new();
//! let changed = tracker.update("1. Read the file\n2. Edit the code\n");
//! assert!(changed);
//! assert_eq!(tracker.entries().len(), 2);
//! ```

use agent_client_protocol::schema as acp;

/// Parses streamed assistant output for numbered-list plan items and tracks
/// their execution status.
///
/// Only items in the initial assistant response (before the first tool call)
/// are tracked. After a tool call boundary, the tracker enters a
/// `post_tool_call` state and ignores subsequent text.
///
/// Status transitions:
/// - `Pending` when an item is first detected.
/// - `InProgress` when streaming moves past an entry to a later one.
/// - `Completed` after `finalize()` is called at the end of the turn.
///
/// # Examples
///
/// ```
/// use xzatoma::agent::plan_tracker::PlanTracker;
///
/// let mut tracker = PlanTracker::new();
/// assert!(!tracker.has_entries());
///
/// let changed = tracker.update("1. Step one\n");
/// assert!(changed);
/// assert!(tracker.has_entries());
///
/// tracker.finalize();
/// assert_eq!(
///     tracker.entries()[0].status,
///     agent_client_protocol::schema::PlanEntryStatus::Completed
/// );
/// ```
pub struct PlanTracker {
    entries: Vec<acp::PlanEntry>,
    buffer: String,
    post_tool_call: bool,
}

impl Default for PlanTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl PlanTracker {
    /// Create a new tracker with no entries.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::plan_tracker::PlanTracker;
    ///
    /// let tracker = PlanTracker::new();
    /// assert!(!tracker.has_entries());
    /// ```
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            buffer: String::new(),
            post_tool_call: false,
        }
    }

    /// Feed a chunk of streamed assistant text.
    ///
    /// Appends `chunk` to an internal line buffer, then processes all complete
    /// lines (lines terminated by `\n`). Lines matching `^\d+\.\s+.+` are
    /// treated as plan entries. When a new entry is detected, the previous
    /// last entry (if `Pending`) is promoted to `InProgress`.
    ///
    /// Returns `true` if the plan changed (new entries were added or an entry
    /// transitioned from `Pending` to `InProgress`), signalling the caller to
    /// emit a `SessionUpdate::Plan`.
    ///
    /// After `on_tool_call_started()` has been called, this method is a no-op
    /// and always returns `false`.
    ///
    /// # Arguments
    ///
    /// * `chunk` - Incremental text from the streamed assistant response.
    ///
    /// # Returns
    ///
    /// `true` if the plan changed, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::plan_tracker::PlanTracker;
    ///
    /// let mut tracker = PlanTracker::new();
    /// let changed = tracker.update("1. Read the file\n2. Edit the code\n");
    /// assert!(changed);
    /// assert_eq!(tracker.entries().len(), 2);
    /// ```
    pub fn update(&mut self, chunk: &str) -> bool {
        if self.post_tool_call {
            return false;
        }

        self.buffer.push_str(chunk);

        // Only process complete lines (terminated by \n).
        // The last partial line stays in the buffer.
        let last_newline = match self.buffer.rfind('\n') {
            Some(pos) => pos,
            None => return false,
        };

        let process_end = last_newline + 1;
        let to_process = self.buffer[..process_end].to_string();
        self.buffer = self.buffer[process_end..].to_string();

        let mut changed = false;
        for line in to_process.lines() {
            let trimmed = line.trim();
            if !is_numbered_list_item(trimmed) {
                continue;
            }
            let content = extract_item_content(trimmed);
            if content.is_empty() {
                continue;
            }
            // Ignore duplicates.
            if self.entries.iter().any(|e| e.content == content) {
                continue;
            }
            // Promote the current last Pending entry to InProgress before
            // adding the new entry.
            if let Some(last) = self.entries.last_mut() {
                if last.status == acp::PlanEntryStatus::Pending {
                    last.status = acp::PlanEntryStatus::InProgress;
                }
            }
            self.entries.push(acp::PlanEntry::new(
                content,
                acp::PlanEntryPriority::Medium,
                acp::PlanEntryStatus::Pending,
            ));
            changed = true;
        }

        changed
    }

    /// Notify the tracker that a tool call has started.
    ///
    /// After this call, [`update`] silently ignores all text, preventing
    /// tool-output numbered lists from being misidentified as plan items.
    ///
    /// [`update`]: PlanTracker::update
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::plan_tracker::PlanTracker;
    ///
    /// let mut tracker = PlanTracker::new();
    /// tracker.on_tool_call_started();
    /// let changed = tracker.update("1. This should be ignored\n");
    /// assert!(!changed);
    /// assert!(!tracker.has_entries());
    /// ```
    pub fn on_tool_call_started(&mut self) {
        self.post_tool_call = true;
    }

    /// Finalize the plan at the end of a turn.
    ///
    /// Promotes all entries (regardless of current status) to `Completed`.
    /// Returns `true` if at least one entry exists, so the caller can emit a
    /// final `SessionUpdate::Plan` update.
    ///
    /// # Returns
    ///
    /// `true` when the plan has at least one entry, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::plan_tracker::PlanTracker;
    /// use agent_client_protocol::schema::PlanEntryStatus;
    ///
    /// let mut tracker = PlanTracker::new();
    /// tracker.update("1. Step one\n2. Step two\n");
    /// let has_entries = tracker.finalize();
    /// assert!(has_entries);
    /// assert!(tracker.entries().iter().all(|e| e.status == PlanEntryStatus::Completed));
    /// ```
    pub fn finalize(&mut self) -> bool {
        if self.entries.is_empty() {
            return false;
        }
        for entry in &mut self.entries {
            entry.status = acp::PlanEntryStatus::Completed;
        }
        true
    }

    /// Return a snapshot of the current plan entries.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::plan_tracker::PlanTracker;
    ///
    /// let mut tracker = PlanTracker::new();
    /// tracker.update("1. Do something\n");
    /// assert_eq!(tracker.entries().len(), 1);
    /// ```
    pub fn entries(&self) -> &[acp::PlanEntry] {
        &self.entries
    }

    /// Return `true` if any entries have been detected.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::plan_tracker::PlanTracker;
    ///
    /// let mut tracker = PlanTracker::new();
    /// assert!(!tracker.has_entries());
    /// tracker.update("1. A step\n");
    /// assert!(tracker.has_entries());
    /// ```
    pub fn has_entries(&self) -> bool {
        !self.entries.is_empty()
    }

    /// Reset all entries and the internal buffer.
    ///
    /// Called at the start of a new turn to reuse the tracker.
    /// After reset, [`has_entries`] returns `false` and subsequent [`update`]
    /// calls can detect new plan items again.
    ///
    /// [`has_entries`]: PlanTracker::has_entries
    /// [`update`]: PlanTracker::update
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::agent::plan_tracker::PlanTracker;
    ///
    /// let mut tracker = PlanTracker::new();
    /// tracker.update("1. Step\n");
    /// tracker.reset();
    /// assert!(!tracker.has_entries());
    /// ```
    pub fn reset(&mut self) {
        self.entries.clear();
        self.buffer.clear();
        self.post_tool_call = false;
    }
}

/// Returns `true` if `line` matches the pattern `^\d+\.\s+.+`.
///
/// The line must begin with one or more ASCII digits, followed by a period,
/// followed by at least one ASCII whitespace character, followed by at least
/// one non-whitespace character.
fn is_numbered_list_item(line: &str) -> bool {
    if line.is_empty() {
        return false;
    }
    // Must start with at least one digit.
    let digit_count = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_count == 0 {
        return false;
    }
    let after_digits = &line[digit_count..];
    // Must be followed by a period.
    if !after_digits.starts_with('.') {
        return false;
    }
    let after_dot = &after_digits[1..];
    // Must have at least one ASCII whitespace.
    if after_dot.is_empty() || !after_dot.starts_with(|c: char| c.is_ascii_whitespace()) {
        return false;
    }
    // Non-empty content after the whitespace.
    let content = after_dot.trim_start_matches(|c: char| c.is_ascii_whitespace());
    !content.is_empty()
}

/// Extract the text content from a numbered-list item line.
///
/// Strips the leading `N. ` prefix and returns the trimmed remainder.
fn extract_item_content(line: &str) -> String {
    if let Some(dot_pos) = line.find('.') {
        line[dot_pos + 1..].trim_start().to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_tracker_detects_numbered_list_items() {
        let mut tracker = PlanTracker::new();
        let changed = tracker.update("1. Read the file\n2. Edit the code\n");
        assert!(
            changed,
            "update must return true when new entries are detected"
        );
        assert_eq!(
            tracker.entries().len(),
            2,
            "two numbered items must produce two entries"
        );
        assert_eq!(
            tracker.entries()[0].status,
            acp::PlanEntryStatus::InProgress,
            "first entry must be InProgress after second entry is detected"
        );
        assert_eq!(
            tracker.entries()[1].status,
            acp::PlanEntryStatus::Pending,
            "last entry must be Pending"
        );
    }

    #[test]
    fn test_plan_tracker_returns_false_for_plain_text() {
        let mut tracker = PlanTracker::new();
        let changed = tracker.update("This is plain text with no list items.\n");
        assert!(
            !changed,
            "update must return false when no numbered items are found"
        );
        assert!(
            tracker.entries().is_empty(),
            "no entries must be created for plain text"
        );
    }

    #[test]
    fn test_plan_tracker_ignores_text_after_tool_call() {
        let mut tracker = PlanTracker::new();
        tracker.on_tool_call_started();
        let changed = tracker.update("1. Should be ignored\n");
        assert!(
            !changed,
            "update must return false after on_tool_call_started"
        );
        assert!(
            tracker.entries().is_empty(),
            "no entries must be created after tool call boundary"
        );
    }

    #[test]
    fn test_plan_tracker_promotes_entries_to_in_progress() {
        let mut tracker = PlanTracker::new();
        // First item
        tracker.update("1. First step\n");
        assert_eq!(
            tracker.entries()[0].status,
            acp::PlanEntryStatus::Pending,
            "first entry must be Pending before second arrives"
        );
        // Second item causes first to become InProgress
        tracker.update("2. Second step\n");
        assert_eq!(
            tracker.entries()[0].status,
            acp::PlanEntryStatus::InProgress,
            "first entry must become InProgress when second arrives"
        );
        assert_eq!(
            tracker.entries()[1].status,
            acp::PlanEntryStatus::Pending,
            "second entry must be Pending"
        );
    }

    #[test]
    fn test_plan_tracker_finalize_promotes_all_to_completed() {
        let mut tracker = PlanTracker::new();
        tracker.update("1. Step one\n2. Step two\n");
        let has_entries = tracker.finalize();
        assert!(has_entries, "finalize must return true when entries exist");
        assert!(
            tracker
                .entries()
                .iter()
                .all(|e| e.status == acp::PlanEntryStatus::Completed),
            "all entries must be Completed after finalize"
        );
    }

    #[test]
    fn test_plan_tracker_finalize_returns_false_when_no_entries() {
        let mut tracker = PlanTracker::new();
        let has_entries = tracker.finalize();
        assert!(
            !has_entries,
            "finalize must return false when no entries exist"
        );
    }

    #[test]
    fn test_plan_tracker_reset_clears_entries_and_buffer() {
        let mut tracker = PlanTracker::new();
        tracker.update("1. Step one\n");
        tracker.reset();
        assert!(
            !tracker.has_entries(),
            "has_entries must return false after reset"
        );
        assert!(
            tracker.entries().is_empty(),
            "entries must be empty after reset"
        );
        // After reset, new entries can be detected again.
        let changed = tracker.update("1. New step\n");
        assert!(changed, "update must detect new items after reset");
        assert!(
            tracker.has_entries(),
            "entries must exist after reset + update"
        );
    }

    #[test]
    fn test_plan_tracker_ignores_duplicate_items() {
        let mut tracker = PlanTracker::new();
        tracker.update("1. Do something\n");
        tracker.update("1. Do something\n");
        assert_eq!(
            tracker.entries().len(),
            1,
            "duplicate entries must be ignored"
        );
    }
}
