use gpui::{
    App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce,
    SharedString, Styled as _, Window, prelude::FluentBuilder as _, rems, white,
};
use gpui_base::{Spring, Switch as BaseSwitch, SwitchThumb, SwitchTrack, Theme, spring};
use std::time::Duration;

#[derive(IntoElement)]
pub struct Switch {
    base: BaseSwitch,
    id: ElementId,
    checked: bool,
    disabled: bool,
}

impl Switch {
    pub fn new(id: impl Into<ElementId>) -> Self {
        let id = id.into();
        Self {
            base: BaseSwitch::new(id.clone()),
            id,
            checked: false,
            disabled: false,
        }
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.base = self.base.accessibility_label(label);
        self
    }

    pub fn on_change(mut self, handler: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.base = self
            .base
            .on_change(move |next, _, window, cx| handler(next, window, cx));
        self
    }
}

impl RenderOnce for Switch {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = Theme::global(cx).tokens.colors;
        let checked = self.checked;
        let progress = spring(
            (self.id.clone(), "checked"),
            if checked { 1.0 } else { 0.0 },
            Spring::new(Duration::from_millis(160)).with_damping(0.7),
            window,
            cx,
        );
        let color_progress = progress.clamp(0.0, 1.0);
        let (unchecked_color, checked_color, thumb_color) = if self.disabled {
            (
                colors.background.blend(colors.secondary.opacity(0.7)),
                colors.secondary.blend(colors.primary.opacity(0.4)),
                colors.background.blend(white().opacity(0.8)),
            )
        } else {
            (colors.secondary, colors.primary, white())
        };
        let track_color = unchecked_color.blend(checked_color.opacity(color_progress));
        let hover_track_color = colors
            .background
            .blend(track_color.opacity(0.8 + 0.1 * color_progress));
        let hover_group = format!("switch-hover-{}", self.id);

        self.base
            .checked(checked)
            .disabled(self.disabled)
            .h_7()
            .w_9()
            .flex()
            .flex_shrink_0()
            .items_center()
            .rounded_full()
            .group(hover_group.clone())
            .child(
                SwitchTrack::new((self.id, "track"))
                    .checked(checked)
                    .h_5()
                    .w_full()
                    .flex()
                    .items_center()
                    .rounded_full()
                    .p_0p5()
                    .bg(track_color)
                    .when(!self.disabled, |this| {
                        this.group_hover(hover_group, move |style| style.bg(hover_track_color))
                    })
                    .child(
                        SwitchThumb::new(checked)
                            .relative()
                            .left(rems(progress))
                            .size_4()
                            .flex_shrink_0()
                            .rounded_full()
                            .bg(thumb_color)
                            .when(!self.disabled, |this| this.shadow_sm()),
                    ),
            )
    }
}
