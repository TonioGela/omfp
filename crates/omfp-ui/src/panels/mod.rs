//! The panel registry (UI-024): every panel is a kind with typed content and a placement, so new kinds — including
//! views published by agents (08) — fit without restructuring the shell. The shell renders whatever the registry
//! places in each region.

/// Where the shell shows a panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// The right-hand column, as tabs.
    Side,
    /// Below the thread view.
    Bottom,
    /// A tab of the main area, beside the thread.
    Main,
    /// Over the shell.
    Floating,
}

impl Placement {
    pub const ALL: [Self; 4] = [Self::Side, Self::Bottom, Self::Main, Self::Floating];

    #[must_use]
    pub fn token(self) -> &'static str {
        match self {
            Self::Side => "side",
            Self::Bottom => "bottom",
            Self::Main => "main",
            Self::Floating => "floating",
        }
    }

    #[must_use]
    pub fn from_token(token: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|placement| placement.token() == token)
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Side => "Side",
            Self::Bottom => "Bottom",
            Self::Main => "Main",
            Self::Floating => "Floating",
        }
    }
}

/// What a panel shows; each kind carries the data it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelContent {
    /// All workspaces with their threads, git status and PR link (UI-025).
    Workspaces,
}

impl PanelContent {
    /// Stable kind token.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Workspaces => "workspaces",
        }
    }
}

/// One registered panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Panel {
    pub id: String,
    pub title: String,
    pub content: PanelContent,
    pub placement: Placement,
}

/// The panels of this window, in registration order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelRegistry {
    panels: Vec<Panel>,
}

impl Default for PanelRegistry {
    /// The Workspaces panel in the side column.
    fn default() -> Self {
        let mut registry = Self::empty();
        registry.register(Panel {
            id: "workspaces".to_owned(),
            title: "Workspaces".to_owned(),
            content: PanelContent::Workspaces,
            placement: Placement::Side,
        });
        registry
    }
}

impl PanelRegistry {
    #[must_use]
    pub fn empty() -> Self {
        Self { panels: Vec::new() }
    }

    /// Adds `panel`, replacing a panel with the same id in place.
    pub fn register(&mut self, panel: Panel) {
        match self
            .panels
            .iter_mut()
            .find(|existing| existing.id == panel.id)
        {
            Some(existing) => *existing = panel,
            None => self.panels.push(panel),
        }
    }

    /// Removes the panel `id`; `false` when there was none.
    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.panels.len();
        self.panels.retain(|panel| panel.id != id);
        self.panels.len() != before
    }

    /// Moves the panel `id`; `false` when there is none.
    pub fn place(&mut self, id: &str, placement: Placement) -> bool {
        self.panels
            .iter_mut()
            .find(|panel| panel.id == id)
            .map(|panel| panel.placement = placement)
            .is_some()
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Panel> {
        self.panels.iter().find(|panel| panel.id == id)
    }

    /// The panels in `placement`, in registration order.
    #[must_use]
    pub fn in_placement(&self, placement: Placement) -> Vec<&Panel> {
        self.panels
            .iter()
            .filter(|panel| panel.placement == placement)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(panels: &[&Panel]) -> Vec<String> {
        panels.iter().map(|panel| panel.id.clone()).collect()
    }

    // spec: UI-024
    #[test]
    fn panels_are_registered_replaced_moved_and_removed_by_id() {
        let mut registry = PanelRegistry::default();
        assert_eq!(
            ids(&registry.in_placement(Placement::Side)),
            vec!["workspaces".to_owned()]
        );
        registry.register(Panel {
            id: "other".to_owned(),
            title: "Other".to_owned(),
            content: PanelContent::Workspaces,
            placement: Placement::Bottom,
        });
        assert!(registry.place("workspaces", Placement::Bottom));
        assert_eq!(
            ids(&registry.in_placement(Placement::Bottom)),
            vec!["workspaces".to_owned(), "other".to_owned()]
        );
        assert_eq!(registry.in_placement(Placement::Side), Vec::<&Panel>::new());
        registry.register(Panel {
            id: "other".to_owned(),
            title: "Renamed".to_owned(),
            content: PanelContent::Workspaces,
            placement: Placement::Floating,
        });
        assert_eq!(
            registry.get("other").map(|panel| panel.title.as_str()),
            Some("Renamed")
        );
        assert_eq!(
            ids(&registry.in_placement(Placement::Floating)),
            vec!["other".to_owned()]
        );
        assert!(registry.remove("other"));
        assert!(!registry.place("other", Placement::Main));
    }
}
