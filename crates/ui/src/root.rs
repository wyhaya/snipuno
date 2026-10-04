use crate::tooltip::WindowTooltips;
use gpui::{
    App, Context, Div, InteractiveElement as _, IntoElement, Refineable as _, Render, Stateful,
    StyleRefinement, Styled as _, Subscription, Window,
};
use gpui_base::{Root, RootPlugin, TextSelection, Theme};

pub(crate) fn init(cx: &mut App) {
    Root::register_plugin(cx, Presentation::new);
}

pub(crate) fn tooltips(window: &Window, cx: &App) -> Option<WindowTooltips> {
    let root = window.root::<Root>()??;
    let presentation = root.read(cx).plugin::<Presentation>()?;
    Some(presentation.read(cx).tooltips.clone())
}

struct Presentation {
    tooltips: WindowTooltips,
    _activation: Subscription,
}

impl Presentation {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tooltips = WindowTooltips::new(cx);
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() {
                this.tooltips.hide(cx);
            }
        });
        Self {
            tooltips,
            _activation: activation,
        }
    }
}

impl RootPlugin for Presentation {
    fn prepare(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.set_rem_size(Theme::global(cx).tokens.typography.md.size);
        TextSelection::activate_scope(Default::default(), window, cx);
    }

    fn style(&self, surface: &mut Stateful<Div>, _: &mut Window, cx: &mut App) {
        let theme = Theme::global(cx);
        surface.style().refine(
            &StyleRefinement::default()
                .font_family(theme.tokens.typography.sans.clone())
                .bg(theme.tokens.colors.background)
                .text_color(theme.tokens.colors.foreground),
        );
        let tooltips = self.tooltips.clone();
        surface
            .interactivity()
            .capture_any_mouse_down(move |_, _, cx| tooltips.hide(cx));
        let tooltips = self.tooltips.clone();
        surface
            .interactivity()
            .on_scroll_wheel(move |_, _, cx| tooltips.hide(cx));
    }
}

impl Render for Presentation {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.tooltips.overlay.clone()
    }
}
