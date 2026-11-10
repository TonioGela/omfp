//! Transcript virtualization (UI-042, spike S-06): only the items in and around the viewport are in the DOM; the rest
//! is replaced by two spacers sized from measured heights (estimated until measured). While the view is pinned to the
//! bottom, the window is anchored at the end so streaming output stays visible; scrolling up unpins it.

use std::collections::BTreeMap;

/// Height assumed for an item that has not been measured yet.
pub const ESTIMATED_ITEM_PX: f64 = 120.0;
/// Rendered beyond each edge of the viewport.
pub const OVERSCAN_PX: f64 = 800.0;
/// Rendered from the bottom before the viewport is known (first frame, server-side rendering).
pub const INITIAL_PX: f64 = 3000.0;
/// The view counts as pinned within this distance of the bottom.
pub const PIN_THRESHOLD_PX: f64 = 32.0;

/// The scroll container's visible area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub scroll_top: f64,
    pub height: f64,
}

/// The items to render, `start..end`, and the spacer heights replacing the others.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    pub start: usize,
    pub end: usize,
    pub pad_top: f64,
    pub pad_bottom: f64,
}

/// What one transcript list knows about its layout. Kept per thread so switching threads keeps the position.
#[derive(Debug, Clone, PartialEq)]
pub struct ListMetrics {
    /// Measured heights by item key.
    pub heights: BTreeMap<String, f64>,
    pub viewport: Option<Viewport>,
    /// Follow the bottom as content grows.
    pub pinned: bool,
    /// The user holds the scrollbar or touches the list: scroll events reflect their intent.
    pub dragging: bool,
}

impl Default for ListMetrics {
    fn default() -> Self {
        Self {
            heights: BTreeMap::new(),
            viewport: None,
            pinned: true,
            dragging: false,
        }
    }
}

impl ListMetrics {
    fn height(&self, key: &str) -> f64 {
        self.heights.get(key).copied().unwrap_or(ESTIMATED_ITEM_PX)
    }

    /// Records a measured height; `false` when it did not change (no re-render needed).
    pub fn record_height(&mut self, key: &str, height: f64) -> bool {
        if self
            .heights
            .get(key)
            .is_some_and(|known| (known - height).abs() < 0.5)
        {
            return false;
        }
        self.heights.insert(key.to_owned(), height);
        true
    }

    /// A scroll event: the new viewport. Reaching the bottom pins the view; leaving it unpins only while the user
    /// drags, since layout changes (measured heights replacing estimates, growing content) also move the scroll
    /// position.
    pub fn on_scroll(&mut self, scroll_top: f64, scroll_height: f64, client_height: f64) {
        self.viewport = Some(Viewport {
            scroll_top,
            height: client_height,
        });
        if scroll_height - scroll_top - client_height <= PIN_THRESHOLD_PX {
            self.pinned = true;
        } else if self.dragging {
            self.pinned = false;
        }
    }

    /// The user scrolled up (wheel, touch, keys): stop following the bottom.
    pub fn scrolled_up_by_user(&mut self) {
        self.pinned = false;
    }

    /// Where a freshly mounted list scrolls to: the stored position when the user had scrolled up, `None` for the
    /// bottom (pinned, or never scrolled).
    #[must_use]
    pub fn mount_scroll_top(&self) -> Option<f64> {
        self.viewport
            .filter(|_| !self.pinned)
            .map(|viewport| viewport.scroll_top)
    }

    /// A keydown reached the list: a scroll-up key (`ArrowUp`, `PageUp`, `Home`) unpins, unless it was typed into a
    /// form control (an interaction's input, textarea, select or button), where it edits or moves within the control.
    pub fn on_key(&mut self, scroll_up_key: bool, from_control: bool) {
        if scroll_up_key && !from_control {
            self.scrolled_up_by_user();
        }
    }

    /// The window over the items with `keys`, in order.
    #[must_use]
    pub fn window(&self, keys: &[String]) -> Window {
        let heights: Vec<f64> = keys.iter().map(|key| self.height(key)).collect();
        let (start, end) = match self.viewport {
            Some(viewport) if !self.pinned => {
                let top = viewport.scroll_top - OVERSCAN_PX;
                let bottom = viewport.scroll_top + viewport.height + OVERSCAN_PX;
                let mut offset = 0.0;
                let mut start = heights.len();
                let mut end = heights.len();
                for (index, height) in heights.iter().enumerate() {
                    if start == heights.len() && offset + height > top {
                        start = index;
                    }
                    if offset >= bottom {
                        end = index;
                        break;
                    }
                    offset += height;
                }
                (start.min(heights.len().saturating_sub(1)), end)
            }
            viewport => {
                let budget = viewport.map_or(INITIAL_PX, |viewport| viewport.height + OVERSCAN_PX);
                let mut used = 0.0;
                let mut start = heights.len();
                while start > 0 && used < budget {
                    start -= 1;
                    used += heights[start];
                }
                (start, heights.len())
            }
        };
        let end = end.max(start);
        Window {
            start,
            end,
            pad_top: heights[..start].iter().sum(),
            pad_bottom: heights[end..].iter().sum(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(count: usize) -> Vec<String> {
        (0..count).map(|index| format!("m{index}")).collect()
    }

    // spec: UI-042
    #[test]
    fn before_measuring_only_the_tail_is_rendered() {
        let metrics = ListMetrics::default();
        let window = metrics.window(&keys(10_000));
        assert_eq!((window.start, window.end), (10_000 - 25, 10_000));
        assert!((window.pad_top - 9_975.0 * ESTIMATED_ITEM_PX).abs() < 1e-6);
        assert!(window.pad_bottom.abs() < 1e-6);
        assert_eq!(
            metrics.window(&[]),
            Window {
                start: 0,
                end: 0,
                pad_top: 0.0,
                pad_bottom: 0.0
            }
        );
    }

    // spec: UI-042
    #[test]
    fn scrolling_up_unpins_and_renders_the_items_around_the_viewport() {
        let keys = keys(1000);
        let mut metrics = ListMetrics::default();
        let total = 1000.0 * ESTIMATED_ITEM_PX;
        metrics.on_scroll(total - 600.0, total, 600.0);
        assert!(metrics.pinned);
        metrics.scrolled_up_by_user();
        metrics.on_scroll(12_000.0, total, 600.0);
        assert!(!metrics.pinned);
        let window = metrics.window(&keys);
        // Items overlapping 11 200..13 400 px: 93 (11 160..11 280) to 111 (13 320..13 440).
        assert_eq!((window.start, window.end), (93, 112));
        assert!(
            (window.pad_top + window.pad_bottom + 19.0 * ESTIMATED_ITEM_PX - total).abs() < 1e-6
        );
        metrics.on_scroll(total - 620.0, total, 600.0);
        assert!(metrics.pinned, "back within the threshold of the bottom");
    }

    // spec: UI-042
    #[test]
    fn layout_shifts_keep_the_view_pinned_but_dragging_away_unpins_it() {
        let mut metrics = ListMetrics::default();
        metrics.on_scroll(1_000.0, 4_000.0, 600.0);
        assert!(
            metrics.pinned,
            "a scroll event caused by layout, not by the user"
        );
        metrics.dragging = true;
        metrics.on_scroll(1_000.0, 4_000.0, 600.0);
        assert!(!metrics.pinned);
        metrics.on_scroll(3_400.0, 4_000.0, 600.0);
        assert!(metrics.pinned, "dragged back to the bottom");
    }

    // spec: UI-042
    #[test]
    fn measured_heights_replace_estimates_and_pinned_windows_follow_new_items() {
        let mut metrics = ListMetrics::default();
        assert!(metrics.record_height("m9", 2_000.0));
        assert!(!metrics.record_height("m9", 2_000.2));
        metrics.on_scroll(0.0, 600.0, 600.0);
        let window = metrics.window(&keys(10));
        assert_eq!(
            (window.start, window.end),
            (9, 10),
            "the tall last item fills viewport and overscan"
        );
        let grown = metrics.window(&keys(11));
        assert_eq!((grown.start, grown.end), (9, 11));
        assert!((grown.pad_top - 9.0 * ESTIMATED_ITEM_PX).abs() < 1e-6);
    }

    // spec: UI-042
    #[test]
    fn a_remounted_list_returns_to_the_bottom_or_to_where_the_user_scrolled() {
        let mut metrics = ListMetrics::default();
        assert_eq!(metrics.mount_scroll_top(), None);
        metrics.on_scroll(400.0, 5000.0, 600.0);
        metrics.scrolled_up_by_user();
        assert_eq!(metrics.mount_scroll_top(), Some(400.0));
        metrics.on_scroll(4400.0, 5000.0, 600.0);
        assert_eq!(metrics.mount_scroll_top(), None);
    }

    // spec: UI-042
    #[test]
    fn scroll_keys_typed_into_a_control_keep_the_list_pinned() {
        let mut metrics = ListMetrics::default();
        metrics.on_key(true, true);
        metrics.on_key(false, false);
        assert!(metrics.pinned);
        metrics.on_key(true, false);
        assert!(!metrics.pinned);
    }
}
