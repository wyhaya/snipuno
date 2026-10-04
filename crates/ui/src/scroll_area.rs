use gpui::{
    App, Div, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, RenderOnce,
    ScrollHandle, StatefulInteractiveElement as _, Styled as _, Window, div,
};
use gpui_base::{InteractiveElementExt as _, Scrollbar};

#[derive(IntoElement)]
pub struct ScrollArea {
    id: ElementId,
    content: Div,
}

impl ScrollArea {
    pub fn new(id: impl Into<ElementId>, content: Div) -> Self {
        Self {
            id: id.into(),
            content,
        }
    }
}

impl RenderOnce for ScrollArea {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let scroll_handle = window
            .use_keyed_state(self.id.clone(), cx, |_, _| ScrollHandle::default())
            .read(cx)
            .clone();
        let content = self
            .content
            .id((self.id.clone(), "content"))
            .flex_none()
            .h_auto()
            .min_h_full();

        div()
            .id(self.id.clone())
            .size_full()
            .relative()
            .overflow_y_hidden()
            .child(
                div()
                    .id((self.id.clone(), "area"))
                    .size_full()
                    .flex()
                    .flex_col()
                    .overflow_y_scroll()
                    .lock_scroll_axis()
                    .track_scroll(&scroll_handle)
                    .child(content),
            )
            .child(
                div().absolute().inset_0().child(
                    Scrollbar::vertical(&scroll_handle)
                        .id((self.id, "scrollbar"))
                        .viewport_from_layout(),
                ),
            )
    }
}
