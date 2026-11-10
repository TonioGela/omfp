//! pmo UI (docs/specs/04-ui.md). Built with `dx` for the `web` or `desktop` platform.

use dioxus::prelude::*;

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        main { "data-testid": "app", "pmo" }
    }
}
