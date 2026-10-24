//! A zero-size widget that turns the input method on and keeps its
//! candidate window at the text cursor.
//!
//! iced only enables the platform IME while some widget asks for it, from
//! its `update` during a redraw request (`Shell::request_input_method`); a
//! `canvas` cannot, having no `Shell`. This widget sits in a `stack` above
//! the grid's canvas, draws nothing, and asks on every redraw. Composed text
//! then arrives as `input_method::Event::Commit` (see `crate::keys`).

use iced::advanced::input_method::{InputMethod, Purpose};
use iced::advanced::layout::{self, Layout};
use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{Clipboard, Shell, mouse, renderer};
use iced::{Element, Event, Length, Rectangle, Size, window};

/// The widget: `cursor` is the text cursor's cell, in window coordinates.
pub struct ImeAnchor {
    cursor: Rectangle,
}

impl ImeAnchor {
    /// An anchor keeping the IME window at `cursor`.
    #[must_use]
    pub fn new(cursor: Rectangle) -> Self {
        Self { cursor }
    }
}

impl<Message, Theme, Renderer: renderer::Renderer> Widget<Message, Theme, Renderer> for ImeAnchor {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Shrink, Length::Shrink)
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &Renderer,
        _limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(Size::ZERO)
    }

    fn draw(
        &self,
        _tree: &Tree,
        _renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
    }

    fn update(
        &mut self,
        _tree: &mut Tree,
        event: &Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        if let Event::Window(window::Event::RedrawRequested(_)) = event {
            shell.request_input_method::<&str>(&InputMethod::Enabled {
                cursor: self.cursor,
                purpose: Purpose::Terminal,
                preedit: None,
            });
        }
    }
}

impl<'a, Message, Theme, Renderer: renderer::Renderer + 'a> From<ImeAnchor>
    for Element<'a, Message, Theme, Renderer>
{
    fn from(anchor: ImeAnchor) -> Self {
        Element::new(anchor)
    }
}
